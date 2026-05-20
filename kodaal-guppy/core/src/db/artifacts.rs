impl Database {
    pub fn attach_artifact(
        &mut self,
        prompt_id: &str,
        request: AttachArtifactRequest,
    ) -> Result<Artifact, ApiError> {
        let prompt = self.get_prompt(prompt_id)?;
        validate_storage_mode(&request.storage_mode)?;
        let path = PathBuf::from(request.path.trim());
        let path_text = path.to_string_lossy().to_string();
        if let Some(existing) = self
            .conn
            .query_row(
                "SELECT id FROM artifacts WHERE prompt_id = ?1 AND original_path = ?2",
                params![prompt_id, path_text],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            let artifact = self.get_artifact(&existing)?;
            if artifact.storage_mode == request.storage_mode {
                return Ok(artifact);
            }
            return self.update_artifact_storage(
                &existing,
                StorageModePatch {
                    storage_mode: request.storage_mode,
                },
            );
        }
        self.insert_artifact(
            prompt_id,
            prompt.project_id,
            &path,
            &request.storage_mode,
            "manual",
        )
    }

    pub fn link_auto_artifact(
        &mut self,
        prompt_id: &str,
        path: &Path,
    ) -> Result<Option<Artifact>, ApiError> {
        let prompt = self.get_prompt(prompt_id)?;
        let project_id = prompt.project_id;
        let path_text = path.to_string_lossy().to_string();
        if let Some(existing) = self
            .conn
            .query_row(
                "SELECT id FROM artifacts WHERE prompt_id = ?1 AND original_path = ?2",
                params![prompt_id, path_text],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(Some(self.get_artifact(&existing)?));
        }
        if !path.exists() || !path.is_file() {
            return Ok(None);
        }
        self.insert_artifact(prompt_id, project_id, path, "reference", "auto_watch")
            .map(Some)
    }

    fn insert_artifact(
        &mut self,
        prompt_id: &str,
        project_id: Option<String>,
        path: &Path,
        storage_mode: &str,
        detection_mode: &str,
    ) -> Result<Artifact, ApiError> {
        if !path.exists() || !path.is_file() {
            return Err(ApiError::not_found(
                "FILE_NOT_FOUND",
                "artifact file not found",
            ));
        }
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ApiError::invalid_payload("artifact path must include filename", Some("path"))
            })?
            .to_string();
        let (snapshot_blob, snapshot_size) = if storage_mode == "snapshot" {
            let bytes = read_limited_file(path)?;
            let size = bytes.len() as i64;
            (Some(bytes), Some(size))
        } else {
            (None, None)
        };
        let id = ids::uuid();
        self.conn.execute(
            "INSERT INTO artifacts (id, prompt_id, filename, original_path, project_id, storage_mode, snapshot_blob, snapshot_size, detection_mode, is_broken)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            params![
                id,
                prompt_id,
                filename,
                path.to_string_lossy().to_string(),
                project_id,
                storage_mode,
                snapshot_blob,
                snapshot_size,
                detection_mode,
            ],
        )?;
        self.get_artifact(&id)
    }

    pub fn delete_artifact(&mut self, prompt_id: &str, artifact_id: &str) -> Result<(), ApiError> {
        let changed = self.conn.execute(
            "DELETE FROM artifacts WHERE id = ?1 AND prompt_id = ?2",
            params![artifact_id, prompt_id],
        )?;
        if changed == 0 {
            return Err(ApiError::not_found(
                "ARTIFACT_NOT_FOUND",
                "artifact not found",
            ));
        }
        Ok(())
    }

    pub fn artifact_content(&mut self, artifact_id: &str) -> Result<(Vec<u8>, String), ApiError> {
        let (artifact, snapshot_blob): (Artifact, Option<Vec<u8>>) =
            self.artifact_with_blob(artifact_id)?;
        if let Some(blob) = snapshot_blob {
            return Ok((
                blob,
                artifact
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
            ));
        }
        let path = PathBuf::from(&artifact.original_path);
        let bytes = fs::read(&path).map_err(|_| {
            let _ = self.mark_artifact_broken(artifact_id);
            ApiError::new(
                http::StatusCode::GONE,
                "ARTIFACT_BROKEN",
                "artifact source file is gone",
                None,
            )
        })?;
        Ok((bytes, "application/octet-stream".to_string()))
    }

    pub fn update_artifact_storage(
        &mut self,
        artifact_id: &str,
        request: StorageModePatch,
    ) -> Result<Artifact, ApiError> {
        validate_storage_mode(&request.storage_mode)?;
        let artifact = self.get_artifact(artifact_id)?;
        if request.storage_mode == "snapshot" {
            let bytes = read_limited_file(Path::new(&artifact.original_path))?;
            self.conn.execute(
                "UPDATE artifacts SET storage_mode = 'snapshot', snapshot_blob = ?1, snapshot_size = ?2, is_broken = 0, last_verified_at = ?3 WHERE id = ?4",
                params![bytes, bytes.len() as i64, ids::now_iso(), artifact_id],
            )?;
        } else {
            if !Path::new(&artifact.original_path).exists() {
                return Err(ApiError::new(
                    http::StatusCode::GONE,
                    "ARTIFACT_BROKEN",
                    "cannot switch to reference mode because original file is gone",
                    None,
                ));
            }
            self.conn.execute(
                "UPDATE artifacts SET storage_mode = 'reference', snapshot_blob = NULL, snapshot_size = NULL, is_broken = 0, last_verified_at = ?1 WHERE id = ?2",
                params![ids::now_iso(), artifact_id],
            )?;
        }
        self.get_artifact(artifact_id)
    }

    pub fn verify_artifact_links(&mut self) -> Result<ArtifactVerification, ApiError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original_path, is_broken
             FROM artifacts
             WHERE storage_mode = 'reference' AND snapshot_blob IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? == 1,
            ))
        })?;
        let mut targets = Vec::new();
        for row in rows {
            targets.push(row?);
        }
        drop(stmt);

        let mut checked = 0_i64;
        let mut broken = 0_i64;
        let mut repaired = 0_i64;
        let now = ids::now_iso();
        for (id, original_path, was_broken) in targets {
            checked += 1;
            let exists = Path::new(&original_path).is_file();
            if exists {
                if was_broken {
                    repaired += 1;
                }
                self.conn.execute(
                    "UPDATE artifacts SET is_broken = 0, last_verified_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
            } else {
                broken += 1;
                self.conn.execute(
                    "UPDATE artifacts SET is_broken = 1, last_verified_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
            }
        }
        Ok(ArtifactVerification {
            checked,
            broken,
            repaired,
        })
    }

    pub fn copy_artifact(
        &mut self,
        artifact_id: &str,
        request: CopyArtifactRequest,
    ) -> Result<CopyArtifactResponse, ApiError> {
        match request.on_conflict.as_str() {
            "prompt" | "overwrite" | "rename" | "skip" => {}
            _ => {
                return Err(ApiError::invalid_payload(
                    "on_conflict must be prompt, overwrite, rename, or skip",
                    Some("on_conflict"),
                ))
            }
        }
        let project = self.get_project(&request.target_project_id)?;
        let project_path = project.path.ok_or_else(|| {
            ApiError::new(
                http::StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "target project has no filesystem path",
                None,
            )
        })?;
        if project_path.starts_with("domain://") {
            return Err(ApiError::new(
                http::StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "target project is not a filesystem project",
                None,
            ));
        }
        let artifact = self.get_artifact(artifact_id)?;
        let (bytes, _) = self.artifact_content(artifact_id)?;
        let target_dir = PathBuf::from(project_path);
        let mut target_path = target_dir.join(&artifact.filename);
        let mut renamed_from = None;
        if target_path.exists() {
            match request.on_conflict.as_str() {
                "prompt" => {
                    return Err(ApiError::conflict(
                        "FILE_EXISTS",
                        "target file already exists",
                    ))
                }
                "skip" => {
                    return Ok(CopyArtifactResponse {
                        copied: false,
                        target_path: target_path.to_string_lossy().to_string(),
                        renamed_from: None,
                    })
                }
                "overwrite" => {}
                "rename" => {
                    renamed_from = Some(artifact.filename.clone());
                    target_path = next_available_path(&target_path);
                }
                _ => unreachable!(),
            }
        }
        fs::create_dir_all(&target_dir).map_err(|error| ApiError::internal(error.to_string()))?;
        fs::write(&target_path, bytes).map_err(|error| ApiError::internal(error.to_string()))?;
        Ok(CopyArtifactResponse {
            copied: true,
            target_path: target_path.to_string_lossy().to_string(),
            renamed_from,
        })
    }

}

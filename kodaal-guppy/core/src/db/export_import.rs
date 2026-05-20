impl Database {
    pub fn export_json(&self, query: PromptQuery) -> Result<Value, ApiError> {
        let prompts = self.prompts_for_export(query)?;
        let projects = self.list_projects()?;
        let tags = self.list_tags()?;
        Ok(serde_json::json!({
            "prompts": prompts,
            "projects": projects,
            "tags": tags
        }))
    }

    pub fn export_markdown(&self, query: PromptQuery) -> Result<String, ApiError> {
        let prompts = self.prompts_for_export(query)?;
        let mut output = String::from("# Kodaal Guppy Export\n\n");
        for prompt in prompts {
            output.push_str(&format!(
                "## Prompt {}\n\n- Source: {}/{}\n- Created: {}\n- Uses: {}\n\n{}\n\n",
                prompt.id,
                prompt.source,
                prompt.source_app,
                prompt.created_at,
                prompt.use_count,
                prompt.text
            ));
        }
        Ok(output)
    }

    pub fn import_json_bytes(&mut self, bytes: &[u8]) -> Result<ImportSummary, ApiError> {
        let value: Value = serde_json::from_slice(bytes)?;
        let mut counts = ImportCounts::default();
        if let Some(projects) = value.get("projects").and_then(Value::as_array) {
            for project in projects {
                let object = project.as_object().ok_or_else(|| {
                    ApiError::invalid_payload(
                        "each imported project must be an object",
                        Some("projects"),
                    )
                })?;
                let id = object
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| {
                        ApiError::invalid_payload("imported project missing id", Some("id"))
                    })?;
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| {
                        ApiError::invalid_payload("imported project missing name", Some("name"))
                    })?;
                let path = object.get("path").and_then(Value::as_str);
                let color = object
                    .get("color")
                    .and_then(Value::as_str)
                    .unwrap_or("#2b9c9c");
                let created_at = object
                    .get("created_at")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
                    .unwrap_or_else(ids::now_iso);
                let changed = self.conn.execute(
                    "INSERT OR IGNORE INTO projects (id, name, path, color, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, name, path, color, created_at],
                )?;
                counts.projects += i64::try_from(changed).unwrap_or(0);
            }
        }
        if let Some(tags) = value.get("tags").and_then(Value::as_array) {
            for tag in tags {
                let object = tag.as_object().ok_or_else(|| {
                    ApiError::invalid_payload("each imported tag must be an object", Some("tags"))
                })?;
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| {
                        ApiError::invalid_payload("imported tag missing name", Some("name"))
                    })?;
                let (_, inserted) = self.ensure_tag_row(name)?;
                if inserted {
                    counts.tags += 1;
                }
            }
        }
        let prompts = value
            .get("prompts")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ApiError::invalid_payload("import JSON must contain prompts array", Some("prompts"))
            })?
            .clone();
        for prompt in prompts {
            let object = prompt.as_object().ok_or_else(|| {
                ApiError::invalid_payload("each imported prompt must be an object", Some("prompts"))
            })?;
            let text = object.get("text").and_then(Value::as_str).ok_or_else(|| {
                ApiError::invalid_payload("imported prompt missing text", Some("text"))
            })?;
            let source = object
                .get("source")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ApiError::invalid_payload("imported prompt missing source", Some("source"))
                })?;
            let source_app = object
                .get("source_app")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ApiError::invalid_payload(
                        "imported prompt missing source_app",
                        Some("source_app"),
                    )
                })?;
            let metadata = object
                .get("metadata")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let response = self.ingest_prompt(CapturePayload {
                text: text.to_string(),
                source: source.to_string(),
                source_app: source_app.to_string(),
                project_hint: None,
                conversation_id: object
                    .get("conversation_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                conversation_title: object
                    .get("conversation_title")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                metadata: Some(metadata),
            })?;
            let project_id = object
                .get("project_id")
                .and_then(Value::as_str)
                .filter(|id| self.get_project(id).is_ok());
            let favorite = object
                .get("favorite")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let use_count = object
                .get("use_count")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 1)
                .unwrap_or(1);
            let created_at = object.get("created_at").and_then(Value::as_str);
            let last_used_at = object.get("last_used_at").and_then(Value::as_str);
            self.conn.execute(
                "UPDATE prompts
                 SET project_id = ?1,
                     favorite = ?2,
                     use_count = ?3,
                     created_at = COALESCE(?4, created_at),
                     last_used_at = COALESCE(?5, last_used_at)
                 WHERE id = ?6",
                params![
                    project_id,
                    if favorite { 1 } else { 0 },
                    use_count,
                    created_at,
                    last_used_at,
                    response.id
                ],
            )?;
            if let Some(tags) = object.get("tags").and_then(Value::as_array) {
                for tag in tags.iter().filter_map(Value::as_str) {
                    let (tag_id, inserted) = self.ensure_tag_row(tag)?;
                    if inserted {
                        counts.tags += 1;
                    }
                    self.conn.execute(
                        "INSERT OR IGNORE INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)",
                        params![response.id, tag_id],
                    )?;
                }
            }
            counts.prompts += 1;
        }
        Ok(ImportSummary { imported: counts })
    }

    fn validate_capture(&self, payload: &CapturePayload) -> Result<(), ApiError> {
        if payload.text.is_empty() {
            return Err(ApiError::invalid_payload(
                "text must not be empty",
                Some("text"),
            ));
        }
        if payload.text.chars().count() > self.max_prompt_length as usize {
            return Err(ApiError::too_large(
                "PROMPT_TOO_LARGE",
                "prompt text exceeds max length",
            ));
        }
        validate_source(&payload.source)?;
        if payload.source_app.is_empty() || payload.source_app.len() > 64 {
            return Err(ApiError::invalid_payload(
                "source_app must be 1-64 chars",
                Some("source_app"),
            ));
        }
        if !payload
            .source_app
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        {
            return Err(ApiError::invalid_payload(
                "source_app may contain only letters, digits, dot, underscore, and dash",
                Some("source_app"),
            ));
        }
        if let Some(hint) = payload.project_hint.as_ref() {
            validate_project_hint(hint)?;
        }
        Ok(())
    }

    fn resolve_project(&mut self, hint: &ProjectHint) -> Result<String, ApiError> {
        validate_project_hint(hint)?;
        let path = match hint.kind.as_str() {
            "domain" => format!("domain://{}", hint.value.trim().to_lowercase()),
            "path" | "cwd" => hint.value.trim().to_string(),
            _ => unreachable!(),
        };
        if let Some(id) = self
            .conn
            .query_row(
                "SELECT id FROM projects WHERE path = ?1",
                params![path],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let id = ids::uuid();
        let name = match hint.kind.as_str() {
            "domain" => hint.value.trim().to_lowercase(),
            _ => Path::new(hint.value.trim())
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .unwrap_or(hint.value.trim())
                .to_string(),
        };
        self.conn.execute(
            "INSERT INTO projects (id, name, path) VALUES (?1, ?2, ?3)",
            params![id, name, path],
        )?;
        Ok(id)
    }

    fn prompt_filters(
        &self,
        query: &PromptQuery,
    ) -> Result<(String, Vec<SqlValue>, bool), ApiError> {
        let mut clauses = Vec::new();
        let mut values = Vec::<SqlValue>::new();
        let mut fts = false;
        if let Some(q) = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            if let Some(match_query) = fts_query(q) {
                fts = true;
                clauses.push("prompts_fts MATCH ?");
                values.push(SqlValue::Text(match_query));
            } else {
                clauses.push("lower(p.text) LIKE ?");
                values.push(SqlValue::Text(format!("%{}%", q.to_lowercase())));
            }
        }
        if let Some(value) = query.project_id.as_ref() {
            clauses.push("p.project_id = ?");
            values.push(SqlValue::Text(value.clone()));
        }
        if let Some(value) = query.source.as_ref() {
            validate_source(value)?;
            clauses.push("p.source = ?");
            values.push(SqlValue::Text(value.clone()));
        }
        if let Some(value) = query.source_app.as_ref() {
            clauses.push("p.source_app = ?");
            values.push(SqlValue::Text(value.clone()));
        }
        if let Some(value) = query.favorite.as_ref() {
            let favorite = match value.as_str() {
                "true" => 1,
                "false" => 0,
                _ => {
                    return Err(ApiError::invalid_query(
                        "favorite must be true or false",
                        Some("favorite"),
                    ))
                }
            };
            clauses.push("p.favorite = ?");
            values.push(SqlValue::Integer(favorite));
        }
        if let Some(value) = query.from.as_ref() {
            validate_iso_like(value, "from")?;
            clauses.push("p.created_at >= ?");
            values.push(SqlValue::Text(value.clone()));
        }
        if let Some(value) = query.to.as_ref() {
            validate_iso_like(value, "to")?;
            clauses.push("p.created_at < ?");
            values.push(SqlValue::Text(value.clone()));
        }
        if let Some(value) = query.tag.as_ref() {
            clauses.push("EXISTS (SELECT 1 FROM prompt_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.prompt_id = p.id AND t.name = ? COLLATE NOCASE)");
            values.push(SqlValue::Text(value.clone()));
        }
        let where_sql = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };
        Ok((where_sql, values, fts))
    }

    fn tags_for_prompt(&self, prompt_id: &str) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM tags t JOIN prompt_tags pt ON pt.tag_id = t.id WHERE pt.prompt_id = ?1 ORDER BY t.name ASC",
        )?;
        let rows = stmt.query_map(params![prompt_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }

    fn artifacts_for_prompt(
        &self,
        prompt_id: &str,
    ) -> Result<Vec<ArtifactSummary>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, filename, project_id, storage_mode, is_broken FROM artifacts WHERE prompt_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![prompt_id], |row| {
            Ok(ArtifactSummary {
                id: row.get(0)?,
                filename: row.get(1)?,
                project_id: row.get(2)?,
                storage_mode: row.get(3)?,
                is_broken: row.get::<_, i64>(4)? == 1,
            })
        })?;
        rows.collect()
    }

    fn get_artifact(&self, artifact_id: &str) -> Result<Artifact, ApiError> {
        self.conn
            .query_row(
                "SELECT id, prompt_id, filename, original_path, project_id, storage_mode, snapshot_size, mime_type, detection_mode, is_broken, created_at, last_verified_at
                 FROM artifacts WHERE id = ?1",
                params![artifact_id],
                artifact_from_row,
            )
            .optional()?
            .ok_or_else(|| ApiError::not_found("ARTIFACT_NOT_FOUND", "artifact not found"))
    }

    fn artifact_with_blob(
        &self,
        artifact_id: &str,
    ) -> Result<(Artifact, Option<Vec<u8>>), ApiError> {
        self.conn
            .query_row(
                "SELECT id, prompt_id, filename, original_path, project_id, storage_mode, snapshot_size, mime_type, detection_mode, is_broken, created_at, last_verified_at, snapshot_blob
                 FROM artifacts WHERE id = ?1",
                params![artifact_id],
                |row| Ok((artifact_from_row(row)?, row.get(12)?)),
            )
            .optional()?
            .ok_or_else(|| ApiError::not_found("ARTIFACT_NOT_FOUND", "artifact not found"))
    }

    fn mark_artifact_broken(&mut self, artifact_id: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE artifacts SET is_broken = 1, last_verified_at = ?1 WHERE id = ?2",
            params![ids::now_iso(), artifact_id],
        )?;
        Ok(())
    }

    fn map_counts(&self, sql: &str) -> Result<Map<String, Value>, ApiError> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut map = Map::new();
        for row in rows {
            let (key, count) = row?;
            map.insert(key, Value::from(count));
        }
        Ok(map)
    }

    fn rows_as_objects(&self, sql: &str) -> Result<Vec<Value>, ApiError> {
        let mut stmt = self.conn.prepare(sql)?;
        let names = stmt
            .column_names()
            .into_iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut rows = stmt.query([])?;
        let mut output = Vec::new();
        while let Some(row) = rows.next()? {
            let mut object = Map::new();
            for (index, name) in names.iter().enumerate() {
                object.insert(name.clone(), sql_value_to_json(row.get_ref(index)?));
            }
            output.push(Value::Object(object));
        }
        Ok(output)
    }

    fn prompts_for_export(&self, query: PromptQuery) -> Result<Vec<Prompt>, ApiError> {
        let (where_sql, values, fts) = self.prompt_filters(&query)?;
        let base_from = if fts {
            "FROM prompts p JOIN prompts_fts f ON f.rowid = p.rowid LEFT JOIN projects pr ON pr.id = p.project_id"
        } else {
            "FROM prompts p LEFT JOIN projects pr ON pr.id = p.project_id"
        };
        let sql = format!(
            "SELECT p.id, p.text, p.source, p.source_app, p.project_id, pr.name, p.conversation_id, p.conversation_title, p.use_count, p.favorite, p.metadata, p.created_at, p.last_used_at, p.redacted, p.redaction_reason
             {base_from} {where_sql} ORDER BY p.created_at DESC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| self.prompt_from_row(row))?;
        let mut prompts = Vec::new();
        for row in rows {
            let mut prompt = row?;
            prompt.tags = self.tags_for_prompt(&prompt.id)?;
            prompt.artifacts = self.artifacts_for_prompt(&prompt.id)?;
            prompts.push(prompt);
        }
        Ok(prompts)
    }

    fn prompt_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<Prompt> {
        let metadata_text: String = row.get(10)?;
        let metadata = serde_json::from_str::<Value>(&metadata_text)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        Ok(Prompt {
            id: row.get(0)?,
            text: row.get(1)?,
            source: row.get(2)?,
            source_app: row.get(3)?,
            project_id: row.get(4)?,
            project_name: row.get(5)?,
            conversation_id: row.get(6)?,
            conversation_title: row.get(7)?,
            use_count: row.get(8)?,
            favorite: row.get::<_, i64>(9)? == 1,
            tags: Vec::new(),
            artifacts: Vec::new(),
            metadata,
            redacted: row.get::<_, i64>(13)? == 1,
            redaction_reason: row.get(14)?,
            created_at: row.get(11)?,
            last_used_at: row.get(12)?,
        })
    }
}

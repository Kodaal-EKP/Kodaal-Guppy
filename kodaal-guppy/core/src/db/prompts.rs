impl Database {
    pub fn ingest_prompt(
        &mut self,
        mut payload: CapturePayload,
    ) -> Result<CaptureResponse, ApiError> {
        self.validate_capture(&payload)?;
        let redaction = redact_sensitive_content(&payload.text);
        payload.text = redaction.text;
        let text_hash = ids::sha256_hex(&payload.text);
        let cutoff = (Utc::now() - Duration::seconds(self.dedup_window_seconds as i64))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        if let Some((id, use_count, project_id)) = self
            .conn
            .query_row(
                "SELECT id, use_count, project_id
                 FROM prompts
                 WHERE text_hash = ?1 AND source = ?2 AND source_app = ?3 AND created_at >= ?4
                 ORDER BY created_at DESC LIMIT 1",
                params![text_hash, payload.source, payload.source_app, cutoff],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<String>>(2)?)),
            )
            .optional()?
        {
            let new_count = use_count + 1;
            self.conn.execute(
                "UPDATE prompts SET use_count = ?1, last_used_at = ?2 WHERE id = ?3",
                params![new_count, ids::now_iso(), id],
            )?;
            return Ok(CaptureResponse {
                id,
                deduped: true,
                use_count: new_count,
                project_id,
            });
        }

        let project_id = if let Some(hint) = payload.project_hint.as_ref() {
            Some(self.resolve_project(hint)?)
        } else {
            None
        };
        let id = ids::uuid();
        let now = ids::now_iso();
        let metadata = Value::Object(payload.metadata.unwrap_or_default()).to_string();
        self.conn.execute(
            "INSERT INTO prompts (id, text, text_hash, source, source_app, project_id, conversation_id, conversation_title, metadata, created_at, last_used_at, redacted, redaction_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id,
                payload.text,
                text_hash,
                payload.source,
                payload.source_app,
                project_id,
                payload.conversation_id,
                payload.conversation_title,
                metadata,
                now,
                now,
                if redaction.redacted { 1 } else { 0 },
                redaction.reason
            ],
        )?;
        Ok(CaptureResponse {
            id,
            deduped: false,
            use_count: 1,
            project_id,
        })
    }

    pub fn list_prompts(&self, query: PromptQuery) -> Result<PromptList, ApiError> {
        let (where_sql, values, fts) = self.prompt_filters(&query)?;
        let limit = query.limit.unwrap_or(50);
        let offset = query.offset.unwrap_or(0);
        if !(1..=200).contains(&limit) {
            return Err(ApiError::invalid_query(
                "limit must be 1-200",
                Some("limit"),
            ));
        }
        if offset < 0 {
            return Err(ApiError::invalid_query(
                "offset must be >= 0",
                Some("offset"),
            ));
        }
        let sort = match query.sort.as_deref().unwrap_or("created_desc") {
            "created_desc" => "p.created_at DESC",
            "created_asc" => "p.created_at ASC",
            "use_count_desc" => "p.use_count DESC",
            "last_used_desc" => "p.last_used_at DESC",
            _ => return Err(ApiError::invalid_query("invalid sort", Some("sort"))),
        };
        let base_from = if fts {
            "FROM prompts p JOIN prompts_fts f ON f.rowid = p.rowid LEFT JOIN projects pr ON pr.id = p.project_id"
        } else {
            "FROM prompts p LEFT JOIN projects pr ON pr.id = p.project_id"
        };
        let count_sql = format!("SELECT COUNT(*) {base_from} {where_sql}");
        let total: i64 =
            self.conn
                .query_row(&count_sql, params_from_iter(values.clone()), |row| {
                    row.get(0)
                })?;

        let sql = format!(
            "SELECT p.id, p.text, p.source, p.source_app, p.project_id, pr.name, p.conversation_id, p.conversation_title, p.use_count, p.favorite, p.metadata, p.created_at, p.last_used_at, p.redacted, p.redaction_reason
             {base_from} {where_sql} ORDER BY {sort} LIMIT ? OFFSET ?"
        );
        let mut page_values = values;
        page_values.push(SqlValue::Integer(limit));
        page_values.push(SqlValue::Integer(offset));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(page_values), |row| {
            self.prompt_from_row(row)
        })?;
        let mut items = Vec::new();
        for row in rows {
            let mut prompt = row?;
            prompt.tags = self.tags_for_prompt(&prompt.id)?;
            prompt.artifacts = self.artifacts_for_prompt(&prompt.id)?;
            items.push(prompt);
        }
        Ok(PromptList {
            items,
            total,
            limit,
            offset,
        })
    }

    pub fn get_prompt(&self, id: &str) -> Result<Prompt, ApiError> {
        let mut prompt = self
            .conn
            .query_row(
                "SELECT p.id, p.text, p.source, p.source_app, p.project_id, pr.name, p.conversation_id, p.conversation_title, p.use_count, p.favorite, p.metadata, p.created_at, p.last_used_at, p.redacted, p.redaction_reason
                 FROM prompts p LEFT JOIN projects pr ON pr.id = p.project_id WHERE p.id = ?1",
                params![id],
                |row| self.prompt_from_row(row),
            )
            .optional()?
            .ok_or_else(|| ApiError::not_found("PROMPT_NOT_FOUND", "prompt not found"))?;
        prompt.tags = self.tags_for_prompt(id)?;
        prompt.artifacts = self.artifacts_for_prompt(id)?;
        Ok(prompt)
    }

    pub fn update_prompt(&mut self, id: &str, update: UpdatePrompt) -> Result<Prompt, ApiError> {
        if update.favorite.is_none() && update.conversation_title.is_none() {
            return Err(ApiError::invalid_payload("no update fields provided", None));
        }
        if let Some(favorite) = update.favorite {
            self.conn.execute(
                "UPDATE prompts SET favorite = ?1 WHERE id = ?2",
                params![if favorite { 1 } else { 0 }, id],
            )?;
        }
        if let Some(title) = update.conversation_title {
            if let Some(value) = title.as_ref() {
                if value.len() > 200 {
                    return Err(ApiError::invalid_payload(
                        "conversation_title max length is 200",
                        Some("conversation_title"),
                    ));
                }
            }
            self.conn.execute(
                "UPDATE prompts SET conversation_title = ?1 WHERE id = ?2",
                params![title, id],
            )?;
        }
        self.get_prompt(id)
    }

    pub fn delete_prompt(&mut self, id: &str) -> Result<(), ApiError> {
        let changed = self
            .conn
            .execute("DELETE FROM prompts WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(ApiError::not_found("PROMPT_NOT_FOUND", "prompt not found"));
        }
        Ok(())
    }

    pub fn reuse_prompt(&mut self, id: &str) -> Result<(i64, String), ApiError> {
        let prompt = self.get_prompt(id)?;
        let use_count = prompt.use_count + 1;
        let last_used_at = ids::now_iso();
        self.conn.execute(
            "UPDATE prompts SET use_count = ?1, last_used_at = ?2 WHERE id = ?3",
            params![use_count, last_used_at, id],
        )?;
        Ok((use_count, last_used_at))
    }

    pub fn add_tag(&mut self, prompt_id: &str, name: &str) -> Result<Prompt, ApiError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::invalid_payload(
                "tag name must not be empty",
                Some("name"),
            ));
        }
        if name.len() > 64 {
            return Err(ApiError::invalid_payload(
                "tag name max length is 64",
                Some("name"),
            ));
        }
        self.get_prompt(prompt_id)?;
        let (tag_id, _) = self.ensure_tag_row(name)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO prompt_tags (prompt_id, tag_id) VALUES (?1, ?2)",
            params![prompt_id, tag_id],
        )?;
        self.get_prompt(prompt_id)
    }

    fn ensure_tag_row(&mut self, name: &str) -> Result<(String, bool), ApiError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::invalid_payload(
                "tag name must not be empty",
                Some("name"),
            ));
        }
        if name.len() > 64 {
            return Err(ApiError::invalid_payload(
                "tag name max length is 64",
                Some("name"),
            ));
        }
        if let Some(existing) = self
            .conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok((existing, false));
        }
        let tag_id = ids::uuid();
        self.conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1, ?2)",
            params![tag_id, name],
        )?;
        Ok((tag_id, true))
    }

    pub fn remove_tag(&mut self, prompt_id: &str, tag_id: &str) -> Result<(), ApiError> {
        self.conn.execute(
            "DELETE FROM prompt_tags WHERE prompt_id = ?1 AND tag_id = ?2",
            params![prompt_id, tag_id],
        )?;
        Ok(())
    }

    pub fn list_tags(&self) -> Result<Vec<Tag>, ApiError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.name, COUNT(pt.prompt_id) AS count
             FROM tags t LEFT JOIN prompt_tags pt ON pt.tag_id = t.id
             GROUP BY t.id, t.name ORDER BY count DESC, t.name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                count: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(ApiError::from)
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, ApiError> {
        let mut stmt = self.conn.prepare(
            "SELECT pr.id, pr.name, pr.path, pr.color, COUNT(p.id) AS prompt_count, pr.created_at
             FROM projects pr LEFT JOIN prompts p ON p.project_id = pr.id
             GROUP BY pr.id, pr.name, pr.path, pr.color, pr.created_at
             ORDER BY prompt_count DESC, pr.name ASC",
        )?;
        let rows = stmt.query_map([], project_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(ApiError::from)
    }

    pub fn get_project(&self, id: &str) -> Result<Project, ApiError> {
        self.conn
            .query_row(
                "SELECT pr.id, pr.name, pr.path, pr.color, COUNT(p.id) AS prompt_count, pr.created_at
                 FROM projects pr LEFT JOIN prompts p ON p.project_id = pr.id
                 WHERE pr.id = ?1
                 GROUP BY pr.id, pr.name, pr.path, pr.color, pr.created_at",
                params![id],
                project_from_row,
            )
            .optional()?
            .ok_or_else(|| ApiError::not_found("PROJECT_NOT_FOUND", "project not found"))
    }

    pub fn update_project(&mut self, id: &str, update: UpdateProject) -> Result<Project, ApiError> {
        if update.name.is_none() && update.color.is_none() {
            return Err(ApiError::invalid_payload("no update fields provided", None));
        }
        if let Some(name) = update.name {
            let name = name.trim();
            if name.is_empty() || name.len() > 200 {
                return Err(ApiError::invalid_payload(
                    "project name must be 1-200 chars",
                    Some("name"),
                ));
            }
            self.conn.execute(
                "UPDATE projects SET name = ?1 WHERE id = ?2",
                params![name, id],
            )?;
        }
        if let Some(color) = update.color {
            if !valid_color(&color) {
                return Err(ApiError::invalid_payload(
                    "color must be #RRGGBB",
                    Some("color"),
                ));
            }
            self.conn.execute(
                "UPDATE projects SET color = ?1 WHERE id = ?2",
                params![color, id],
            )?;
        }
        self.get_project(id)
    }

    pub fn delete_project(&mut self, id: &str) -> Result<(), ApiError> {
        let changed = self
            .conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(ApiError::not_found(
                "PROJECT_NOT_FOUND",
                "project not found",
            ));
        }
        Ok(())
    }

    pub fn prune(&mut self, request: PruneRequest) -> Result<PruneResponse, ApiError> {
        if request.older_than.is_none()
            && request.shorter_than.is_none()
            && request.project_id.is_none()
            && request.source.is_none()
        {
            return Err(ApiError::invalid_payload(
                "prune requires at least one criterion",
                None,
            ));
        }
        if let Some(source) = request.source.as_deref() {
            validate_source(source)?;
        }
        let mut clauses = Vec::new();
        let mut values = Vec::<SqlValue>::new();
        if let Some(value) = request.older_than {
            validate_iso_like(&value, "older_than")?;
            clauses.push("created_at < ?");
            values.push(SqlValue::Text(value));
        }
        if let Some(value) = request.shorter_than {
            if value <= 0 {
                return Err(ApiError::invalid_payload(
                    "shorter_than must be positive",
                    Some("shorter_than"),
                ));
            }
            clauses.push("length(text) < ?");
            values.push(SqlValue::Integer(value));
        }
        if let Some(value) = request.project_id {
            clauses.push("project_id = ?");
            values.push(SqlValue::Text(value));
        }
        if let Some(value) = request.source {
            clauses.push("source = ?");
            values.push(SqlValue::Text(value));
        }
        let where_sql = format!("WHERE {}", clauses.join(" AND "));
        let count_sql = format!("SELECT COUNT(*) FROM prompts {where_sql}");
        let deleted: i64 =
            self.conn
                .query_row(&count_sql, params_from_iter(values.clone()), |row| {
                    row.get(0)
                })?;
        if !request.dry_run {
            let delete_sql = format!("DELETE FROM prompts {where_sql}");
            self.conn.execute(&delete_sql, params_from_iter(values))?;
        }
        Ok(PruneResponse {
            deleted,
            dry_run: request.dry_run,
        })
    }

}

struct RedactionResult {
    text: String,
    redacted: bool,
    reason: Option<String>,
}

fn redact_sensitive_content(text: &str) -> RedactionResult {
    let mut output = String::with_capacity(text.len());
    let mut token = String::new();
    let mut reasons = Vec::<&'static str>::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':') {
            token.push(ch);
        } else {
            push_redacted_token(&mut output, &mut token, &mut reasons);
            output.push(ch);
        }
    }
    push_redacted_token(&mut output, &mut token, &mut reasons);
    reasons.sort_unstable();
    reasons.dedup();
    RedactionResult {
        text: output,
        redacted: !reasons.is_empty(),
        reason: (!reasons.is_empty()).then(|| reasons.join(",")),
    }
}

fn push_redacted_token(output: &mut String, token: &mut String, reasons: &mut Vec<&'static str>) {
    if token.is_empty() {
        return;
    }
    if looks_like_jwt(token) {
        output.push_str("[REDACTED:jwt]");
        reasons.push("jwt");
    } else if looks_like_api_key(token) {
        output.push_str("[REDACTED:api-key]");
        reasons.push("api-key");
    } else if looks_like_aws_access_key(token) {
        output.push_str("[REDACTED:aws-access-key]");
        reasons.push("aws-access-key");
    } else {
        output.push_str(token);
    }
    token.clear();
}

fn looks_like_jwt(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts[0].len() >= 10
        && parts[1].len() >= 10
        && parts[2].len() >= 10
        && parts.iter().all(|part| {
            part.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        })
}

fn looks_like_api_key(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    (lower.starts_with("sk-") && value.len() >= 24)
        || (value.starts_with("ghp_") && value.len() >= 24)
        || (value.starts_with("github_pat_") && value.len() >= 30)
        || (value.starts_with("xoxb-") && value.len() >= 24)
}

fn looks_like_aws_access_key(value: &str) -> bool {
    value.len() == 20
        && (value.starts_with("AKIA") || value.starts_with("ASIA"))
        && value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
}

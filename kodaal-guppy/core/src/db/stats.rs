impl Database {
    pub fn reset_statistics(&mut self, audit_log_path: &Path) -> Result<(), ApiError> {
        self.conn
            .execute("UPDATE prompts SET use_count = 1", [])
            .map_err(ApiError::from)?;
        fs::write(audit_log_path, "").map_err(|error| {
            ApiError::internal(format!("failed to clear audit log: {error}"))
        })?;
        Ok(())
    }

    pub fn stats(&self, audit_log_path: &Path, range: &str) -> Result<Value, ApiError> {
        let scope = StatsScope::new(range)?;
        let prompt_filter = scope.prompt_filter();
        let prompt_join_filter = scope.prompt_join_filter();
        let total_prompts = self.count(&format!("SELECT COUNT(*) FROM prompts {prompt_filter}"))?;
        let total_projects = self.count(&format!(
            "SELECT COUNT(DISTINCT project_id) FROM prompts WHERE project_id IS NOT NULL{}",
            scope.and_created_at()
        ))?;
        let artifact_filter = scope.prompt_alias_filter("p");
        let total_artifacts = self.count(&format!(
            "SELECT COUNT(*)
             FROM artifacts a
             JOIN prompts p ON p.id = a.prompt_id
             {artifact_filter}"
        ))?;
        let projects_with_prompts =
            self.count(&format!("SELECT COUNT(DISTINCT project_id) FROM prompts WHERE project_id IS NOT NULL{}", scope.and_created_at()))?;
        let total_copied_from_rows = self.count(&format!(
            "SELECT COALESCE(SUM(use_count - 1), 0) FROM prompts {prompt_filter}"
        ))?;
        let audit = audit_counts(audit_log_path, scope.cutoff.as_deref());
        let total_copied = total_copied_from_rows.max(audit.copied);
        let average_prompts_per_day = ratio(
            total_prompts,
            self.count(&format!(
                "SELECT COUNT(DISTINCT substr(created_at, 1, 10)) FROM prompts {prompt_filter}"
            ))?,
        );
        let average_prompts_per_project = ratio(total_prompts, projects_with_prompts);
        let average_estimated_tokens_per_prompt = self
            .conn
            .query_row(
                &format!(
                    "SELECT COALESCE(AVG((length(text) + 3) / 4.0), 0.0) FROM prompts {prompt_filter}"
                ),
                [],
                |row| row.get::<_, f64>(0),
            )?;
        let by_source = self.map_counts(&format!(
            "SELECT source, COUNT(*) FROM prompts {prompt_filter} GROUP BY source"
        ))?;
        let by_source_app = self.rows_as_objects(
            &format!(
                "SELECT source, source_app, COUNT(*) AS count
             FROM prompts
             {prompt_filter}
             GROUP BY source, source_app
             ORDER BY count DESC, source_app ASC"
            ),
        )?;
        let most_used_platform = by_source_app
            .first()
            .cloned()
            .unwrap_or_else(|| serde_json::json!({"source": null, "source_app": null, "count": 0}));
        let bucket = scope.bucket_expr();
        let overall_time_series = self.rows_as_objects(&format!(
            "SELECT bucket, COUNT(*) AS count
             FROM (
               SELECT {bucket} AS bucket
               FROM prompts
               {prompt_filter}
             )
             GROUP BY bucket
             ORDER BY bucket ASC"
        ))?;
        let platform_time_series = self.rows_as_objects(&format!(
            "SELECT bucket, source, COUNT(*) AS count
             FROM (
               SELECT {bucket} AS bucket, source
               FROM prompts
               {prompt_filter}
             )
             GROUP BY bucket, source
             ORDER BY bucket ASC, source ASC"
        ))?;
        let provider_time_series = self.rows_as_objects(&format!(
            "SELECT bucket, source, source_app, COUNT(*) AS count
             FROM (
               SELECT {bucket} AS bucket, source, source_app
               FROM prompts
               {prompt_filter}
             )
             GROUP BY bucket, source, source_app
             ORDER BY bucket ASC, count DESC, source_app ASC"
        ))?;
        let top_use_count = self.rows_as_objects(
            &format!(
                "SELECT id, text, use_count
             FROM prompts
             {prompt_filter}
             ORDER BY use_count DESC, created_at DESC
             LIMIT 10"
            ),
        )?;
        let top_projects = self.rows_as_objects(
            &format!(
                "SELECT pr.id, pr.name, COUNT(p.id) AS prompt_count, COALESCE(SUM(p.use_count - 1), 0) AS copied_count
             FROM projects pr
             LEFT JOIN prompts p ON p.project_id = pr.id {prompt_join_filter}
             GROUP BY pr.id, pr.name
             ORDER BY prompt_count DESC, pr.name ASC
             LIMIT 10"
            ),
        )?;
        Ok(serde_json::json!({
            "range": scope.name,
            "total_prompts": total_prompts,
            "total_projects": total_projects,
            "total_artifacts": total_artifacts,
            "total_copied": total_copied,
            "total_deleted": audit.deleted,
            "average_prompts_per_day": average_prompts_per_day,
            "average_prompts_per_project": average_prompts_per_project,
            "average_estimated_tokens_per_prompt": round_one(average_estimated_tokens_per_prompt),
            "most_used_platform": most_used_platform,
            "by_source": by_source,
            "by_source_app": by_source_app,
            "overall_time_series": overall_time_series,
            "platform_time_series": platform_time_series,
            "provider_time_series": provider_time_series,
            "by_day_last_30": overall_time_series,
            "top_use_count": top_use_count,
            "top_projects": top_projects
        }))
    }

    fn count(&self, sql: &str) -> Result<i64, ApiError> {
        self.conn
            .query_row(sql, [], |row| row.get(0))
            .map_err(ApiError::from)
    }
}

struct StatsScope {
    name: &'static str,
    cutoff: Option<String>,
}

impl StatsScope {
    fn new(range: &str) -> Result<Self, ApiError> {
        let now = Utc::now();
        match range {
            "" | "all" => Ok(Self {
                name: "all",
                cutoff: None,
            }),
            "day" => Ok(Self {
                name: "day",
                cutoff: Some((now - Duration::days(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            }),
            "week" => Ok(Self {
                name: "week",
                cutoff: Some((now - Duration::days(7)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            }),
            "month" => Ok(Self {
                name: "month",
                cutoff: Some((now - Duration::days(30)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            }),
            "year" => Ok(Self {
                name: "year",
                cutoff: Some((now - Duration::days(365)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
            }),
            _ => Err(ApiError::invalid_query("range must be all, day, week, month, or year", Some("range"))),
        }
    }

    fn prompt_filter(&self) -> String {
        self.cutoff
            .as_ref()
            .map(|cutoff| format!("WHERE created_at >= '{}'", escape_sql_literal(cutoff)))
            .unwrap_or_default()
    }

    fn and_created_at(&self) -> String {
        self.cutoff
            .as_ref()
            .map(|cutoff| format!(" AND created_at >= '{}'", escape_sql_literal(cutoff)))
            .unwrap_or_default()
    }

    fn prompt_join_filter(&self) -> String {
        self.cutoff
            .as_ref()
            .map(|cutoff| format!("AND p.created_at >= '{}'", escape_sql_literal(cutoff)))
            .unwrap_or_default()
    }

    fn prompt_alias_filter(&self, alias: &str) -> String {
        self.cutoff
            .as_ref()
            .map(|cutoff| format!("WHERE {alias}.created_at >= '{}'", escape_sql_literal(cutoff)))
            .unwrap_or_default()
    }

    fn bucket_expr(&self) -> &'static str {
        match self.name {
            "day" => "substr(created_at, 1, 13) || ':00'",
            "year" | "all" => "substr(created_at, 1, 7)",
            _ => "substr(created_at, 1, 10)",
        }
    }
}

#[derive(Default)]
struct AuditCounts {
    copied: i64,
    deleted: i64,
}

fn audit_counts(path: &Path, cutoff: Option<&str>) -> AuditCounts {
    let Ok(text) = fs::read_to_string(path) else {
        return AuditCounts::default();
    };
    let mut counts = AuditCounts::default();
    for line in text.lines() {
        if cutoff.is_some_and(|value| !audit_line_in_range(line, value)) {
            continue;
        }
        if line.contains(" prompt_reuse ") {
            counts.copied += 1;
        }
        if line.contains(" prompt_delete ") {
            counts.deleted += 1;
        }
        if line.contains(" prune ") && line.contains("dry_run=false") {
            counts.deleted += metric_value(line, "deleted=");
        }
    }
    counts
}

fn audit_line_in_range(line: &str, cutoff: &str) -> bool {
    line.get(0..cutoff.len())
        .is_some_and(|timestamp| timestamp >= cutoff)
}

fn escape_sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn metric_value(line: &str, key: &str) -> i64 {
    let Some(rest) = line.split(key).nth(1) else {
        return 0;
    };
    rest.chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<i64>()
        .unwrap_or(0)
}

fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator <= 0 {
        0.0
    } else {
        round_one(numerator as f64 / denominator as f64)
    }
}

fn round_one(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

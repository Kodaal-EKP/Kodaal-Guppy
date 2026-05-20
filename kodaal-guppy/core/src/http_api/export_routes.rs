#[derive(serde::Deserialize)]
struct StatsQuery {
    #[serde(default = "default_stats_range")]
    range: String,
}

fn default_stats_range() -> String {
    "all".to_string()
}

async fn stats(
    State(state): State<AppState>,
    Query(query): Query<StatsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        state
            .db
            .lock()
            .map_err(|_| ApiError::internal("db lock poisoned"))?
            .stats(&state.paths.audit_log_path, &query.range)?,
    ))
}

async fn reset_statistics(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .reset_statistics(&state.paths.audit_log_path)?;
    append_audit(&state, "stats_reset".to_string())?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
struct ExportQuery {
    #[serde(default)]
    format: Option<String>,
    #[serde(flatten)]
    filters: PromptQuery,
}

async fn export_data(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let format = query.format.unwrap_or_else(|| "json".to_string());
    match format.as_str() {
        "json" => {
            let body = state
                .db
                .lock()
                .map_err(|_| ApiError::internal("db lock poisoned"))?
                .export_json(query.filters)?;
            let bytes = serde_json::to_vec_pretty(&body)
                .map_err(|error| ApiError::internal(error.to_string()))?;
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            headers.insert(
                CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=\"kodaal-guppy-export.json\""),
            );
            Ok((headers, bytes).into_response())
        }
        "markdown" => {
            let body = state
                .db
                .lock()
                .map_err(|_| ApiError::internal("db lock poisoned"))?
                .export_markdown(query.filters)?;
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/markdown; charset=utf-8"),
            );
            headers.insert(
                CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=\"kodaal-guppy-export.md\""),
            );
            Ok((headers, body).into_response())
        }
        _ => Err(ApiError::invalid_query(
            "format must be json or markdown",
            Some("format"),
        )),
    }
}

async fn import_data(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<crate::db::ImportSummary>, ApiError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| ApiError::invalid_payload(error.to_string(), None))?
    {
        if field.name() == Some("file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|error| ApiError::invalid_payload(error.to_string(), None))?;
            let summary = state
                .db
                .lock()
                .map_err(|_| ApiError::internal("db lock poisoned"))?
                .import_json_bytes(&bytes)?;
            append_audit(
                &state,
                format!("import prompts={}", summary.imported.prompts),
            )?;
            return Ok(Json(summary));
        }
    }
    Err(ApiError::invalid_payload(
        "multipart form must include file field",
        Some("file"),
    ))
}

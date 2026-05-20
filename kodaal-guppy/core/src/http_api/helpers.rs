fn parse_json<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(ApiError::from)
}

fn is_blocklisted(state: &AppState, payload: &CapturePayload) -> bool {
    let capture = match state.capture.lock() {
        Ok(capture) => capture,
        Err(_) => return false,
    };
    if capture
        .blocklist
        .source_apps
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(&payload.source_app))
    {
        return true;
    }
    if let Some(hint) = payload.project_hint.as_ref() {
        let list = match hint.kind.as_str() {
            "domain" => &capture.blocklist.domains,
            "path" | "cwd" => &capture.blocklist.paths,
            _ => &capture.blocklist.domains,
        };
        return list
            .iter()
            .any(|entry| hint.value.eq_ignore_ascii_case(entry) || hint.value.contains(entry));
    }
    false
}

fn apply_blocklist_delta(
    values: &mut Vec<String>,
    add: Vec<String>,
    remove: Vec<String>,
) -> Result<(), ApiError> {
    for value in add {
        validate_blocklist_value(&value)?;
        if !values.iter().any(|existing| existing == &value) {
            values.push(value);
        }
    }
    for value in remove {
        values.retain(|existing| existing != &value);
    }
    Ok(())
}

fn validate_blocklist_value(value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains("..")
        || value.chars().any(|ch| ch.is_control())
    {
        return Err(ApiError::invalid_payload("invalid blocklist entry", None));
    }
    Ok(())
}

fn append_audit(state: &AppState, event: String) -> Result<(), ApiError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&state.paths.audit_log_path)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    writeln!(file, "{} {}", crate::ids::now_iso(), event)
        .map_err(|error| ApiError::internal(error.to_string()))
}

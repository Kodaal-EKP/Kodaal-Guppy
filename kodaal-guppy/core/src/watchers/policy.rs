fn blocklisted_payload(
    state: &AppState,
    payload: &CapturePayload,
) -> Result<bool, Box<dyn std::error::Error>> {
    let capture = state.capture.lock().map_err(|_| "capture lock poisoned")?;
    if capture
        .blocklist
        .source_apps
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(&payload.source_app))
    {
        return Ok(true);
    }
    Ok(false)
}

fn blocklisted_path(state: &AppState, path: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let capture = state.capture.lock().map_err(|_| "capture lock poisoned")?;
    Ok(capture
        .blocklist
        .paths
        .iter()
        .any(|entry| !entry.is_empty() && path.contains(entry)))
}

fn append_audit(state: &AppState, event: String) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&state.paths.audit_log_path)?;
    writeln!(file, "{} {}", ids::now_iso(), event)
}

fn expand_path(value: &str) -> PathBuf {
    let mut text = value.to_string();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home).to_string_lossy().to_string();
        if text == "~" {
            text = home;
        } else if let Some(rest) = text.strip_prefix("~/") {
            text = format!("{home}/{rest}");
        } else if let Some(rest) = text.strip_prefix("~\\") {
            text = format!("{home}\\{rest}");
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        text = text.replace("%APPDATA%", &PathBuf::from(appdata).to_string_lossy());
    }
    PathBuf::from(text)
}

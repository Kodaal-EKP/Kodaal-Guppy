struct ResolvedAttribution {
    source: String,
    source_app: String,
    entrypoint: Option<String>,
}

fn resolve_generic_attribution(
    value: &Value,
    default_source: &str,
    default_source_app: &str,
) -> ResolvedAttribution {
    let entrypoint = string_at(value, &["/entrypoint", "/message/entrypoint", "/payload/entrypoint"]);
    let (source, source_app) = if default_source_app == "claude-code" {
        match entrypoint.as_deref() {
            Some("claude-vscode" | "vscode") => ("ide", "claude-vscode"),
            Some("claude-cursor" | "cursor") => ("ide", "claude-cursor"),
            Some("claude-desktop" | "desktop") => ("desktop", "claude-desktop"),
            Some("claude-code" | "cli") => ("cli", "claude-code"),
            _ => (default_source, default_source_app),
        }
    } else {
        (default_source, default_source_app)
    };
    ResolvedAttribution {
        source: source.to_string(),
        source_app: source_app.to_string(),
        entrypoint,
    }
}

fn normalize_prompt_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || is_internal_context_text(trimmed) {
        return None;
    }
    if trimmed.starts_with("<command-message>") {
        return normalize_command_message(trimmed);
    }
    if trimmed.starts_with("# Files mentioned by the user:") {
        return extract_embedded_user_request(trimmed);
    }
    if trimmed.starts_with("# /") && trimmed.contains("\n## Input") {
        return normalize_slash_command_wrapper(trimmed);
    }
    Some(text.trim_end().to_string())
}

fn is_internal_context_text(text: &str) -> bool {
    text == "Tool loaded."
        || text.starts_with("# AGENTS.md instructions")
        || text.starts_with("<environment_context>")
        || text.starts_with("<system-reminder>")
        || text.starts_with("<INSTRUCTIONS>")
        || text.starts_with("# Model Set Context")
        || text.starts_with("This session is being continued from a previous conversation")
        || text.starts_with("The following is the Codex agent history")
        || text == "<<autonomous-loop-dynamic>>"
        || (text.starts_with("Your task is to create a detailed summary of the conversation so far")
            && text.contains("Do NOT use any tools"))
}

fn normalize_command_message(text: &str) -> Option<String> {
    let command = tag_text(text, "command-name")?;
    let args = tag_text(text, "command-args").unwrap_or_default();
    let normalized = format!("{command} {args}").trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_embedded_user_request(text: &str) -> Option<String> {
    for marker in [
        "## My request for Codex:",
        "## My request for Claude:",
        "## My request:",
    ] {
        if let Some((_, rest)) = text.rsplit_once(marker) {
            let request = rest.trim();
            if !request.is_empty() {
                return Some(request.to_string());
            }
        }
    }
    None
}

fn tag_text(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim().to_string())
}

fn normalize_slash_command_wrapper(text: &str) -> Option<String> {
    let command = text
        .lines()
        .next()?
        .trim_start_matches('#')
        .split_whitespace()
        .next()?;
    if !command.starts_with('/') {
        return None;
    }
    let input = text.rsplit_once("## Input")?.1.trim();
    if input.is_empty() {
        return None;
    }
    if input.starts_with(command) {
        Some(input.to_string())
    } else {
        Some(format!("{command} {input}"))
    }
}

fn bool_at(value: &Value, paths: &[&str]) -> bool {
    paths
        .iter()
        .any(|path| value.pointer(path).and_then(Value::as_bool) == Some(true))
}

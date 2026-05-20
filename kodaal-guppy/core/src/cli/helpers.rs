fn client() -> Result<LocalApiClient, CliFailure> {
    LocalApiClient::from_default_paths().map_err(CliFailure::from)
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string())
    );
}

fn print_prompt_list(items: &[Value], full: bool) {
    for (index, prompt) in items.iter().enumerate() {
        let text = value_str(prompt, "text");
        println!(
            "{}. [{}] [{}/{}] {}",
            index + 1,
            value_str(prompt, "project_name"),
            value_str(prompt, "source"),
            value_str(prompt, "source_app"),
            value_str(prompt, "id")
        );
        println!("   {}", if full { text } else { truncate(&text, 200) });
    }
    println!("{} results.", items.len());
}

fn write_clipboard(text: &str) -> Result<(), CliFailure> {
    let mut command = if cfg!(windows) {
        Command::new("clip")
    } else if cfg!(target_os = "macos") {
        Command::new("pbcopy")
    } else if Command::new("wl-copy")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
    {
        Command::new("wl-copy")
    } else {
        Command::new("xclip")
    };
    let mut child = command
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| CliFailure::generic("clipboard stdin unavailable".to_string()))?
        .write_all(text.as_bytes())
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    let status = child
        .wait()
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(CliFailure::generic("clipboard command failed".to_string()))
    }
}

fn resolve_tag_id(tag: &str) -> Result<String, CliFailure> {
    let tags = client()?.get_json("/api/tags")?;
    let Some(items) = tags.as_array() else {
        return Err(CliFailure::generic("invalid tag list response".to_string()));
    };
    items
        .iter()
        .find(|item| {
            value_str(item, "id") == tag || value_str(item, "name").eq_ignore_ascii_case(tag)
        })
        .map(|item| value_str(item, "id"))
        .ok_or_else(|| CliFailure {
            code: EXIT_GENERIC,
            message: format!("tag not found: {tag}"),
        })
}

fn join_json_array(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "none".to_string())
}

fn service_running() -> bool {
    client()
        .and_then(|client| client.get_json("/healthz").map_err(CliFailure::from))
        .is_ok()
}

fn default_bin_dir() -> PathBuf {
    if cfg!(windows) {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("Kodaal")
            .join("bin")
    } else {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join(".local")
            .join("bin")
    }
}

fn binary_filename() -> &'static str {
    if cfg!(windows) {
        "kodaal.exe"
    } else {
        "kodaal"
    }
}

fn completion_script(shell: CompletionShell) -> &'static str {
    match shell {
        CompletionShell::Bash => include_str!("../completion/bash"),
        CompletionShell::Zsh => include_str!("../completion/zsh"),
        CompletionShell::Fish => include_str!("../completion/fish"),
        CompletionShell::Powershell => include_str!("../completion/powershell"),
    }
}

fn apply_global_flags(cli: &Cli) {
    if let Some(home) = &cli.home {
        env::set_var("KODAAL_HOME", home);
    }
    if let Some(port) = cli.port {
        env::set_var("KODAAL_PORT", port.to_string());
    }
    if let Some(token) = &cli.token {
        env::set_var("KODAAL_TOKEN_FILE", token);
    }
    if let Some(config) = &cli.config {
        env::set_var("KODAAL_CONFIG", config);
    }
    env::set_var("KODAAL_LOG_LEVEL", &cli.log_level);
    if cli.no_color {
        env::set_var("NO_COLOR", "1");
    }
}

fn append_global_args(command: &mut Command, cli: &Cli) {
    if let Some(home) = &cli.home {
        command.args(["--home", &home.display().to_string()]);
    }
    if let Some(port) = cli.port {
        command.args(["--port", &port.to_string()]);
    }
    if let Some(token) = &cli.token {
        command.args(["--token", &token.display().to_string()]);
    }
    if let Some(config) = &cli.config {
        command.args(["--config", &config.display().to_string()]);
    }
}

fn pid_file() -> Result<PathBuf, CliFailure> {
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    Ok(paths.home_dir.join("run").join("kodaal.pid"))
}

fn encode_params(params: &[(&str, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={}", query_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn push_param(params: &mut Vec<(&'static str, String)>, key: &'static str, value: Option<&String>) {
    if let Some(value) = value {
        params.push((key, value.clone()));
    }
}

fn value_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    output.push_str("...");
    output
}

impl CliFailure {
    fn generic(message: String) -> Self {
        Self {
            code: EXIT_GENERIC,
            message,
        }
    }

    fn not_running(message: &str) -> Self {
        Self {
            code: EXIT_NOT_RUNNING,
            message: message.to_string(),
        }
    }
}

impl From<LocalApiError> for CliFailure {
    fn from(error: LocalApiError) -> Self {
        let code = match error.code {
            "MCP_CORE_UNAVAILABLE" | "MCP_CORE_NOT_INSTALLED" => EXIT_NOT_RUNNING,
            "AUTH_TOKEN_INVALID" | "UNAUTHORIZED" => EXIT_AUTH,
            "INVALID_PAYLOAD" | "INVALID_QUERY" => EXIT_BAD_USAGE,
            _ => EXIT_GENERIC,
        };
        Self {
            code,
            message: error.message,
        }
    }
}

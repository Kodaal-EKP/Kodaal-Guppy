fn export_data(args: &ExportArgs) -> Result<i32, CliFailure> {
    let body = client()?.get_text(&format!(
        "/api/export?format={}",
        query_encode(&args.format)
    ))?;
    if let Some(path) = &args.output {
        fs::write(path, body).map_err(|error| CliFailure::generic(error.to_string()))?;
        println!("Exported prompts to {}.", path.display());
    } else {
        print!("{body}");
    }
    Ok(0)
}

fn config_command(cli: &Cli, args: &ConfigArgs) -> Result<i32, CliFailure> {
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    match args.command {
        ConfigCommand::Path => {
            if cli.json {
                print_json(&json!({
                    "home": paths.home_dir.to_string_lossy(),
                    "config": paths.config_path.to_string_lossy(),
                    "token": paths.token_path.to_string_lossy(),
                    "database": paths.db_path.to_string_lossy(),
                    "audit_log": paths.audit_log_path.to_string_lossy(),
                    "backups": paths.backup_dir.to_string_lossy()
                }));
            } else {
                println!("Home:      {}", paths.home_dir.display());
                println!("Config:    {}", paths.config_path.display());
                println!("Token:     {}", paths.token_path.display());
                println!("Database:  {}", paths.db_path.display());
                println!("Audit log: {}", paths.audit_log_path.display());
                println!("Backups:   {}", paths.backup_dir.display());
            }
        }
        ConfigCommand::Show => {
            let config = Config::load_or_create(&paths)
                .map_err(|error| CliFailure::generic(error.to_string()))?;
            if cli.json {
                print_json(
                    &serde_json::to_value(config)
                        .map_err(|error| CliFailure::generic(error.to_string()))?,
                );
            } else {
                let toml = toml::to_string_pretty(&config)
                    .map_err(|error| CliFailure::generic(error.to_string()))?;
                print!("{toml}");
            }
        }
        ConfigCommand::Validate => {
            let config = Config::load_or_create(&paths)
                .map_err(|error| CliFailure::generic(error.to_string()))?;
            config.validate().map_err(CliFailure::generic)?;
            if cli.json {
                print_json(
                    &json!({ "valid": true, "config": paths.config_path.to_string_lossy() }),
                );
            } else {
                println!("Config valid: {}", paths.config_path.display());
            }
        }
    }
    Ok(0)
}

fn rotate_token(cli: &Cli, args: &RotateTokenArgs) -> Result<i32, CliFailure> {
    if !args.force && service_running() {
        return Err(CliFailure {
            code: EXIT_BAD_USAGE,
            message: "service is running; stop it first or pass --force and restart the service immediately".to_string(),
        });
    }
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    paths
        .ensure()
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    let token = crate::auth::generate_token();
    fs::write(&paths.token_path, format!("{token}\n"))
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    crate::paths::set_private_file(&paths.token_path)
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    if cli.json {
        print_json(&json!({ "rotated": true, "token_path": paths.token_path.to_string_lossy() }));
    } else {
        println!("Rotated token at {}.", paths.token_path.display());
        if args.force {
            println!(
                "Restart the running service before issuing API, UI, MCP, browser, or IDE calls."
            );
        }
    }
    Ok(0)
}

fn capture_shell(args: &CaptureShellArgs) -> Result<i32, CliFailure> {
    if args.text.is_empty() {
        return Ok(0);
    }
    let text = args.text.join(" ");
    let cwd = env::current_dir().ok().map(|path| path.to_string_lossy().to_string());
    let mut body = json!({
        "text": text,
        "source": "cli",
        "source_app": args.source_app
    });
    if let Some(cwd) = cwd {
        body["project_hint"] = json!({ "type": "cwd", "value": cwd });
    }
    match client()?.post_json("/api/prompts", body) {
        Ok(_) => Ok(0),
        Err(error) if error.code == "MCP_CORE_UNAVAILABLE" => Ok(0),
        Err(error) if error.code == "CAPTURE_PAUSED" => Ok(0),
        Err(error) if error.code == "FORBIDDEN" => Ok(0),
        Err(error) => Err(CliFailure {
            code: EXIT_GENERIC,
            message: error.to_string(),
        }),
    }
}

fn install_shell_hook(args: &ShellHookArgs) -> Result<i32, CliFailure> {
    let shell = args.shell.unwrap_or_else(default_shell_hook);
    let rc_file = args
        .rc_file
        .clone()
        .unwrap_or_else(|| default_shell_rc(shell));
    let tools = if args.tool.is_empty() {
        vec![
            "claude".to_string(),
            "codex".to_string(),
            "aider".to_string(),
        ]
    } else {
        args.tool.clone()
    };
    for tool in &tools {
        validate_shell_tool_name(tool)?;
    }
    let previous = fs::read_to_string(&rc_file).unwrap_or_default();
    let next = replace_shell_hook_block(&previous, &shell_hook_block(&tools));
    if let Some(parent) = rc_file.parent() {
        fs::create_dir_all(parent).map_err(|error| CliFailure::generic(error.to_string()))?;
    }
    fs::write(&rc_file, next).map_err(|error| CliFailure::generic(error.to_string()))?;
    println!("Installed Kodaal shell hook in {}.", rc_file.display());
    Ok(0)
}

const SHELL_HOOK_START: &str = "# >>> kodaal guppy shell hook >>>";
const SHELL_HOOK_END: &str = "# <<< kodaal guppy shell hook <<<";

fn shell_hook_block(tools: &[String]) -> String {
    let mut block = String::new();
    block.push_str(SHELL_HOOK_START);
    block.push('\n');
    block.push_str("__kodaal_capture_shell() {\n");
    block.push_str("  command kodaal capture-shell \"$1\" \"${@:2}\" >/dev/null 2>&1 || true\n");
    block.push_str("}\n");
    block.push_str("__kodaal_suggest_shell() {\n");
    block.push_str("  [ -t 2 ] || return 0\n");
    block.push_str("  command kodaal suggest --shell-hook --source cli --source-app \"$1\" \"${@:2}\" 1>&2 2>/dev/null || true\n");
    block.push_str("}\n");
    for tool in tools {
        let source_app = shell_source_app(tool);
        block.push_str(&format!(
            "{tool}() {{\n  __kodaal_suggest_shell {source_app} \"$@\"\n  __kodaal_capture_shell {source_app} \"$@\"\n  command {tool} \"$@\"\n}}\n"
        ));
    }
    block.push_str(SHELL_HOOK_END);
    block.push('\n');
    block
}

fn replace_shell_hook_block(existing: &str, block: &str) -> String {
    let mut output = String::new();
    let mut skipping = false;
    for line in existing.lines() {
        if line == SHELL_HOOK_START {
            skipping = true;
            continue;
        }
        if line == SHELL_HOOK_END {
            skipping = false;
            continue;
        }
        if !skipping {
            output.push_str(line);
            output.push('\n');
        }
    }
    while output.ends_with("\n\n") {
        output.pop();
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(block);
    output
}

fn shell_source_app(tool: &str) -> String {
    match tool {
        "claude" => "claude-code".to_string(),
        "codex" => "codex-cli".to_string(),
        "aider" => "aider".to_string(),
        value => format!("{value}-cli"),
    }
}

fn validate_shell_tool_name(value: &str) -> Result<(), CliFailure> {
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return Err(CliFailure {
            code: EXIT_BAD_USAGE,
            message: "shell tool names may contain only letters, digits, underscore, and dash"
                .to_string(),
        });
    }
    Ok(())
}

fn default_shell_hook() -> ShellHook {
    env::var("SHELL")
        .ok()
        .filter(|value| value.contains("zsh"))
        .map(|_| ShellHook::Zsh)
        .unwrap_or(ShellHook::Bash)
}

fn default_shell_rc(shell: ShellHook) -> PathBuf {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    match shell {
        ShellHook::Bash => home.join(".bashrc"),
        ShellHook::Zsh => home.join(".zshrc"),
    }
}

fn install(args: &InstallArgs) -> Result<i32, CliFailure> {
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    paths
        .ensure()
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    let bin_dir = args.bin_dir.clone().unwrap_or_else(default_bin_dir);
    fs::create_dir_all(&bin_dir).map_err(|error| CliFailure::generic(error.to_string()))?;
    let source = env::current_exe().map_err(|error| CliFailure::generic(error.to_string()))?;
    let target = bin_dir.join(binary_filename());
    fs::copy(&source, &target).map_err(|error| CliFailure::generic(error.to_string()))?;
    let native_target = bin_dir.join(native_host_binary_filename());
    fs::copy(&source, &native_target).map_err(|error| CliFailure::generic(error.to_string()))?;
    let manifests = install_native_host_manifests(&paths, &native_target)?;
    println!("Installed {}.", target.display());
    println!("Installed {}.", native_target.display());
    for manifest in manifests {
        println!("Registered native token host at {}.", manifest.display());
    }
    println!("Use `kodaal start --detach` after adding the install directory to PATH.");
    Ok(0)
}

fn uninstall(args: &InstallArgs) -> Result<i32, CliFailure> {
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    let bin_dir = args.bin_dir.clone().unwrap_or_else(default_bin_dir);
    let target = bin_dir.join(binary_filename());
    if target.exists() {
        fs::remove_file(&target).map_err(|error| CliFailure::generic(error.to_string()))?;
        println!("Removed {}.", target.display());
    } else {
        println!("No installed binary found at {}.", target.display());
    }
    let native_target = bin_dir.join(native_host_binary_filename());
    if native_target.exists() {
        fs::remove_file(&native_target).map_err(|error| CliFailure::generic(error.to_string()))?;
        println!("Removed {}.", native_target.display());
    }
    uninstall_native_host_manifests(&paths);
    Ok(0)
}

fn completion(args: &CompletionArgs) -> Result<i32, CliFailure> {
    print!("{}", completion_script(args.shell));
    Ok(0)
}

fn doctor(cli: &Cli) -> Result<i32, CliFailure> {
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    let mut warnings = 0;
    println!("Kodaal doctor");
    warnings += check_path("Config file", &paths.config_path);
    warnings += check_path("Token file", &paths.token_path);
    warnings += check_path("Database", &paths.db_path);
    match status(cli) {
        Ok(_) => println!("  ok API listener reachable"),
        Err(error) => {
            warnings += 1;
            println!("  warn API listener: {}", error.message);
        }
    }
    Ok(if warnings == 0 { 0 } else { EXIT_GENERIC })
}

fn check_path(label: &str, path: &Path) -> i32 {
    if path.exists() {
        println!("  ok {label}: {}", path.display());
        0
    } else {
        println!("  warn {label}: {} missing", path.display());
        1
    }
}

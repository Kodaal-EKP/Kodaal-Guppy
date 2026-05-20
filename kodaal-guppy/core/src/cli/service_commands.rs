async fn execute(cli: Cli) -> Result<i32, CliFailure> {
    match &cli.command {
        Some(Commands::Start(args)) => start(&cli, args.clone()).await,
        Some(Commands::Stop(args)) => stop(args.force),
        Some(Commands::Restart(args)) => {
            let _ = stop(false);
            start(
                &cli,
                StartArgs {
                    detach: true,
                    no_watcher: args.no_watcher,
                },
            )
            .await
        }
        Some(Commands::Status) => status(&cli),
        Some(Commands::Pause(args)) => pause(&cli, args),
        Some(Commands::Resume) => resume(&cli),
        Some(Commands::Search(args)) => search(&cli, args),
        Some(Commands::Suggest(args)) => suggest(&cli, args),
        Some(Commands::Recent(args)) => recent(&cli, args),
        Some(Commands::Show(args)) => show(&cli, &args.id),
        Some(Commands::Copy(args)) => copy_prompt(&cli, &args.id),
        Some(Commands::Delete(args)) => delete_prompt(&cli, &args.id),
        Some(Commands::Favorite(args)) => favorite_prompt(&cli, args),
        Some(Commands::Tag(args)) => tag_prompt(&cli, args),
        Some(Commands::Untag(args)) => untag_prompt(&cli, args),
        Some(Commands::Prune(args)) => prune_prompts(&cli, args),
        Some(Commands::Import(args)) => import_data(&cli, args),
        Some(Commands::Projects(args)) => projects(&cli, args),
        Some(Commands::Tags) => tags(&cli),
        Some(Commands::Stats) => stats(&cli),
        Some(Commands::Blocklist(args)) => blocklist(&cli, args),
        Some(Commands::Artifact(args)) => artifact(&cli, args),
        Some(Commands::Export(args)) => export_data(args),
        Some(Commands::Config(args)) => config_command(&cli, args),
        Some(Commands::RotateToken(args)) => rotate_token(&cli, args),
        Some(Commands::CaptureShell(args)) => capture_shell(args),
        Some(Commands::InstallShellHook(args)) => install_shell_hook(args),
        Some(Commands::NativeTokenHost) => native_token_host(),
        Some(Commands::Install(args)) => install(args),
        Some(Commands::Uninstall(args)) => uninstall(args),
        Some(Commands::Completion(args)) => completion(args),
        Some(Commands::Doctor) => doctor(&cli),
        Some(Commands::McpServer) => match crate::mcp::run_stdio() {
            Ok(()) => Ok(0),
            Err(error) => Err(CliFailure::generic(error.to_string())),
        },
        Some(Commands::Version) => {
            println!("kodaal {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        None => status_or_help(&cli),
    }
}

async fn start(cli: &Cli, args: StartArgs) -> Result<i32, CliFailure> {
    if args.detach {
        return start_detached(cli, &args);
    }
    crate::server::run(!args.no_watcher)
        .await
        .map(|_| 0)
        .map_err(|error| CliFailure {
            code: EXIT_START_FAILED,
            message: error.to_string(),
        })
}

fn start_detached(cli: &Cli, args: &StartArgs) -> Result<i32, CliFailure> {
    let exe = env::current_exe().map_err(|error| CliFailure::generic(error.to_string()))?;
    let paths = AppPaths::resolve().map_err(|error| CliFailure::generic(error.to_string()))?;
    fs::create_dir_all(paths.home_dir.join("run"))
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    let mut command = Command::new(exe);
    append_global_args(&mut command, cli);
    command.arg("start");
    if args.no_watcher {
        command.arg("--no-watcher");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command.spawn().map_err(|error| CliFailure {
        code: EXIT_START_FAILED,
        message: error.to_string(),
    })?;
    fs::write(pid_file()?, child.id().to_string())
        .map_err(|error| CliFailure::generic(error.to_string()))?;
    if cli.json {
        println!(
            "{}",
            json!({ "pid": child.id(), "ui": "http://127.0.0.1:7878/ui" })
        );
    } else {
        println!("Kodaal started in background with pid {}.", child.id());
        println!("Open http://127.0.0.1:7878/ui");
    }
    Ok(0)
}

fn stop(force: bool) -> Result<i32, CliFailure> {
    let pid_path = pid_file()?;
    let pid = fs::read_to_string(&pid_path)
        .map_err(|_| CliFailure::not_running("Kodaal service is not running."))?
        .trim()
        .to_string();
    let status = if cfg!(windows) {
        let mut command = Command::new("taskkill");
        command.args(["/PID", &pid, "/T"]);
        if force {
            command.arg("/F");
        }
        command.status()
    } else {
        Command::new("kill").arg(&pid).status()
    }
    .map_err(|error| CliFailure::generic(error.to_string()))?;
    if !status.success() {
        return Err(CliFailure::not_running("Kodaal service is not running."));
    }
    let _ = fs::remove_file(pid_path);
    println!("Kodaal stopped.");
    Ok(0)
}

fn status_or_help(cli: &Cli) -> Result<i32, CliFailure> {
    match status(cli) {
        Ok(code) => Ok(code),
        Err(error) if error.code == EXIT_NOT_RUNNING => {
            let mut command = Cli::command();
            command
                .print_help()
                .map_err(|io_error| CliFailure::generic(io_error.to_string()))?;
            println!();
            Err(error)
        }
        Err(error) => Err(error),
    }
}

fn status(cli: &Cli) -> Result<i32, CliFailure> {
    let client = client()?;
    let health = client.get_json("/healthz")?;
    let capture = client.get_json("/api/capture/status")?;
    let stats = client.get_json("/api/stats")?;
    if cli.json {
        print_json(&json!({
            "version": health.get("version").cloned().unwrap_or(Value::Null),
            "running": true,
            "api": "http://127.0.0.1:7878",
            "capture_paused": capture.get("paused").cloned().unwrap_or(Value::Bool(false)),
            "stats": stats
        }));
    } else {
        println!("Kodaal Guppy {}", env!("CARGO_PKG_VERSION"));
        println!("  service: running");
        println!("  api:     http://127.0.0.1:7878");
        println!(
            "  capture: {}",
            if capture
                .get("paused")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "paused"
            } else {
                "active"
            }
        );
        println!(
            "  prompts: {}",
            stats
                .get("total_prompts")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
    }
    Ok(0)
}

fn pause(cli: &Cli, args: &PauseArgs) -> Result<i32, CliFailure> {
    let body = args
        .reason
        .as_ref()
        .map(|reason| json!({ "reason": reason }))
        .unwrap_or_else(|| json!({}));
    let result = client()?.post_json("/api/capture/pause", body)?;
    if cli.json {
        print_json(&result);
    } else {
        println!("Capture paused.");
    }
    Ok(0)
}

fn resume(cli: &Cli) -> Result<i32, CliFailure> {
    let result = client()?.post_json("/api/capture/resume", json!({}))?;
    if cli.json {
        print_json(&result);
    } else {
        println!("Capture resumed.");
    }
    Ok(0)
}

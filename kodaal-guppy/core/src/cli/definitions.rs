use crate::{
    config::Config,
    local_api::{query_encode, LocalApiClient, LocalApiError},
    paths::AppPaths,
};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const EXIT_GENERIC: i32 = 1;
const EXIT_BAD_USAGE: i32 = 2;
const EXIT_NOT_RUNNING: i32 = 3;
const EXIT_START_FAILED: i32 = 4;
const EXIT_AUTH: i32 = 5;

#[derive(Debug, Parser)]
#[command(
    name = "kodaal",
    version,
    about = "Kodaal Guppy local prompt workspace"
)]
struct Cli {
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[arg(long, global = true)]
    port: Option<u16>,
    #[arg(long, global = true)]
    token: Option<PathBuf>,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true, default_value = "info")]
    log_level: String,
    #[arg(long, global = true)]
    no_color: bool,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Start(StartArgs),
    Stop(StopArgs),
    Restart(StartArgs),
    Status,
    Pause(PauseArgs),
    Resume,
    Search(SearchArgs),
    Suggest(SuggestArgs),
    Recent(RecentArgs),
    Show(ShowArgs),
    Copy(ShowArgs),
    Delete(ShowArgs),
    Favorite(FavoriteArgs),
    Tag(TagArgs),
    Untag(UntagArgs),
    Prune(PruneArgs),
    Import(ImportArgs),
    Projects(ProjectsArgs),
    Tags,
    Stats,
    Blocklist(BlocklistArgs),
    Artifact(ArtifactArgs),
    Export(ExportArgs),
    Config(ConfigArgs),
    #[command(name = "rotate-token")]
    RotateToken(RotateTokenArgs),
    #[command(name = "capture-shell", hide = true)]
    CaptureShell(CaptureShellArgs),
    #[command(name = "install-shell-hook")]
    InstallShellHook(ShellHookArgs),
    #[command(name = "native-token-host", hide = true)]
    NativeTokenHost,
    Install(InstallArgs),
    Uninstall(InstallArgs),
    Completion(CompletionArgs),
    Doctor,
    #[command(name = "mcp-server")]
    McpServer,
    Version,
}

#[derive(Debug, Args, Clone)]
struct StartArgs {
    #[arg(short, long)]
    detach: bool,
    #[arg(long)]
    no_watcher: bool,
}

#[derive(Debug, Args)]
struct StopArgs {
    #[arg(short, long)]
    force: bool,
}

#[derive(Debug, Args)]
struct PauseArgs {
    #[arg(long)]
    reason: Option<String>,
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: String,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    source: Option<String>,
    #[arg(long = "source-app")]
    source_app: Option<String>,
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    to: Option<String>,
    #[arg(long, default_value_t = 20)]
    limit: u16,
    #[arg(long, default_value = "created_desc")]
    sort: String,
    #[arg(long)]
    favorite: bool,
    #[arg(long)]
    full: bool,
}

#[derive(Debug, Args)]
struct SuggestArgs {
    #[arg(long, default_value = "cli")]
    source: String,
    #[arg(long = "source-app")]
    source_app: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    limit: Option<u16>,
    #[arg(long = "shell-hook", hide = true)]
    shell_hook: bool,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    draft: Vec<String>,
}

#[derive(Debug, Args)]
struct RecentArgs {
    #[arg(default_value_t = 10)]
    n: u16,
}

#[derive(Debug, Args)]
struct ShowArgs {
    id: String,
}

#[derive(Debug, Args)]
struct FavoriteArgs {
    id: String,
    #[arg(long)]
    unset: bool,
}

#[derive(Debug, Args)]
struct TagArgs {
    id: String,
    name: String,
}

#[derive(Debug, Args)]
struct UntagArgs {
    id: String,
    tag: String,
}

#[derive(Debug, Args)]
struct PruneArgs {
    #[arg(long = "older-than")]
    older_than: Option<String>,
    #[arg(long = "shorter-than")]
    shorter_than: Option<i64>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    source: Option<String>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct ImportArgs {
    file: PathBuf,
}

#[derive(Debug, Args)]
struct ProjectsArgs {
    #[command(subcommand)]
    command: Option<ProjectCommand>,
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    List,
    Show {
        id: String,
    },
    Rename {
        id: String,
        name: String,
    },
    Color {
        id: String,
        color: String,
    },
    Delete {
        id: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Args)]
struct BlocklistArgs {
    #[arg(long = "add-domain")]
    add_domain: Vec<String>,
    #[arg(long = "remove-domain")]
    remove_domain: Vec<String>,
    #[arg(long = "add-path")]
    add_path: Vec<String>,
    #[arg(long = "remove-path")]
    remove_path: Vec<String>,
    #[arg(long = "add-source-app")]
    add_source_app: Vec<String>,
    #[arg(long = "remove-source-app")]
    remove_source_app: Vec<String>,
}

#[derive(Debug, Args)]
struct ArtifactArgs {
    #[command(subcommand)]
    command: ArtifactCommand,
}

#[derive(Debug, Subcommand)]
enum ArtifactCommand {
    Attach {
        prompt_id: String,
        path: PathBuf,
        #[arg(long, default_value = "reference")]
        storage_mode: String,
    },
    Delete {
        prompt_id: String,
        artifact_id: String,
    },
    Content {
        artifact_id: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    Copy {
        artifact_id: String,
        target_project_id: String,
        #[arg(long, default_value = "prompt")]
        on_conflict: String,
    },
    Storage {
        artifact_id: String,
        storage_mode: String,
    },
}

#[derive(Debug, Args)]
struct ExportArgs {
    #[arg(long, default_value = "json")]
    format: String,
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Path,
    Show,
    Validate,
}

#[derive(Debug, Args)]
struct RotateTokenArgs {
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct CaptureShellArgs {
    source_app: String,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    text: Vec<String>,
}

#[derive(Debug, Args)]
struct ShellHookArgs {
    #[arg(long, value_enum)]
    shell: Option<ShellHook>,
    #[arg(long = "rc-file")]
    rc_file: Option<PathBuf>,
    #[arg(long = "tool")]
    tool: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ShellHook {
    Bash,
    Zsh,
}

#[derive(Debug, Args)]
struct InstallArgs {
    #[arg(long = "bin-dir")]
    bin_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct CompletionArgs {
    shell: CompletionShell,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

#[derive(Debug)]
struct CliFailure {
    code: i32,
    message: String,
}

pub async fn run() -> i32 {
    if is_native_token_host_invocation() {
        return native_token_host_exit_code();
    }

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() {
                EXIT_BAD_USAGE
            } else {
                0
            };
        }
    };
    apply_global_flags(&cli);
    match execute(cli).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}", error.message);
            error.code
        }
    }
}

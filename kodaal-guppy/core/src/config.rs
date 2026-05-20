use crate::paths::AppPaths;
use serde::{Deserialize, Serialize};
use std::{fs, net::IpAddr};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub prune: PruneConfig,
    #[serde(default)]
    pub suggestions: SuggestionsConfig,
    #[serde(default)]
    pub backups: BackupConfig,
    #[serde(default)]
    pub watchers: WatchersConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    #[serde(default = "default_timeout")]
    pub request_timeout_seconds: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfig {
    #[serde(default = "default_dedup_window")]
    pub dedup_window_seconds: u32,
    #[serde(default = "default_prompt_length")]
    pub max_prompt_length: u32,
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "default_true")]
    pub auto_create_projects: bool,
    #[serde(default)]
    pub blocklist: BlocklistConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    #[serde(default = "default_database_encryption")]
    pub encryption: String,
    #[serde(default = "default_database_key_env")]
    pub key_env: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlocklistConfig {
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub source_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PruneConfig {
    #[serde(default)]
    pub older_than_days: Option<u32>,
    #[serde(default)]
    pub shorter_than: Option<i64>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default = "default_true")]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestionsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub cli_enabled: bool,
    #[serde(default = "default_true")]
    pub ide_enabled: bool,
    #[serde(default = "default_suggestion_min_chars")]
    pub min_chars: u32,
    #[serde(default = "default_suggestion_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupConfig {
    #[serde(default = "default_backup_count")]
    pub keep_count: u32,
    #[serde(default = "default_true")]
    pub pre_migration_backup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchersConfig {
    #[serde(default = "default_claude_code_path")]
    pub claude_code: String,
    #[serde(default = "default_true")]
    pub claude_code_enabled: bool,
    #[serde(default = "default_codex_path")]
    pub codex: String,
    #[serde(default = "default_true")]
    pub codex_enabled: bool,
    #[serde(default = "default_cursor_path")]
    pub cursor: String,
    #[serde(default = "default_true")]
    pub cursor_enabled: bool,
    #[serde(default = "default_aider_path")]
    pub aider: String,
    #[serde(default = "default_true")]
    pub aider_enabled: bool,
    #[serde(default = "default_zed_path")]
    pub zed: String,
    #[serde(default = "default_true")]
    pub zed_enabled: bool,
    #[serde(default = "default_lapce_path")]
    pub lapce: String,
    #[serde(default)]
    pub lapce_enabled: bool,
    #[serde(default = "default_true")]
    pub backfill_on_first_run: bool,
    #[serde(default = "default_backfill_days")]
    pub backfill_days: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            bind_address: default_bind_address(),
            request_timeout_seconds: default_timeout(),
        }
    }
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            dedup_window_seconds: default_dedup_window(),
            max_prompt_length: default_prompt_length(),
            paused: false,
            auto_create_projects: true,
            blocklist: BlocklistConfig::default(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            encryption: default_database_encryption(),
            key_env: default_database_key_env(),
        }
    }
}

impl Default for PruneConfig {
    fn default() -> Self {
        Self {
            older_than_days: None,
            shorter_than: None,
            source: None,
            dry_run: true,
        }
    }
}

impl Default for SuggestionsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cli_enabled: true,
            ide_enabled: true,
            min_chars: default_suggestion_min_chars(),
            limit: default_suggestion_limit(),
        }
    }
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            keep_count: default_backup_count(),
            pre_migration_backup: true,
        }
    }
}

impl Default for WatchersConfig {
    fn default() -> Self {
        Self {
            claude_code: default_claude_code_path(),
            claude_code_enabled: true,
            codex: default_codex_path(),
            codex_enabled: true,
            cursor: default_cursor_path(),
            cursor_enabled: true,
            aider: default_aider_path(),
            aider_enabled: true,
            zed: default_zed_path(),
            zed_enabled: true,
            lapce: default_lapce_path(),
            lapce_enabled: false,
            backfill_on_first_run: true,
            backfill_days: default_backfill_days(),
        }
    }
}

impl Config {
    pub fn load_or_create(paths: &AppPaths) -> Result<Self, Box<dyn std::error::Error>> {
        if paths.config_path.exists() {
            let text = fs::read_to_string(&paths.config_path)?;
            let config: Config = toml::from_str(&text)?;
            Ok(config.with_defaults())
        } else {
            let config = Config::default();
            fs::write(&paths.config_path, config.default_toml())?;
            crate::paths::set_private_file(&paths.config_path)?;
            Ok(config)
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let ip: IpAddr = self
            .server
            .bind_address
            .parse()
            .map_err(|_| "server.bind_address must be an IP address".to_string())?;
        if !ip.is_loopback() {
            return Err("server.bind_address must be loopback-only".to_string());
        }
        if self.server.port == 0 {
            return Err("server.port must be 1-65535".to_string());
        }
        if !(1..=300).contains(&self.server.request_timeout_seconds) {
            return Err("server.request_timeout_seconds must be 1-300".to_string());
        }
        if self.capture.dedup_window_seconds > 3600 {
            return Err("capture.dedup_window_seconds must be <= 3600".to_string());
        }
        if !(1..=10_000_000).contains(&self.capture.max_prompt_length) {
            return Err("capture.max_prompt_length must be 1-10000000".to_string());
        }
        validate_database_config(&self.database)?;
        validate_prune_config(&self.prune)?;
        validate_suggestions_config(&self.suggestions)?;
        if !(1..=100).contains(&self.backups.keep_count) {
            return Err("backups.keep_count must be 1-100".to_string());
        }
        if self.watchers.backfill_days > 365 {
            return Err("watchers.backfill_days must be <= 365".to_string());
        }
        Ok(())
    }

    pub fn save(&self, paths: &AppPaths) -> Result<(), Box<dyn std::error::Error>> {
        self.validate()?;
        let text = toml::to_string_pretty(self)?;
        fs::write(&paths.config_path, text)?;
        crate::paths::set_private_file(&paths.config_path)?;
        Ok(())
    }

    fn with_defaults(self) -> Self {
        self
    }

    fn default_toml(&self) -> String {
        r#"[server]
port = 7878
bind_address = "127.0.0.1"
request_timeout_seconds = 30

[database]
encryption = "off"
key_env = "KODAAL_DB_KEY"

[capture]
dedup_window_seconds = 60
max_prompt_length = 1000000
paused = false
auto_create_projects = true

[capture.blocklist]
domains = []
paths = []
source_apps = []

[prune]
dry_run = true

[suggestions]
enabled = false
cli_enabled = true
ide_enabled = true
min_chars = 24
limit = 3

[backups]
keep_count = 7
pre_migration_backup = true

[watchers]
claude_code = "~/.claude/projects"
claude_code_enabled = true
codex = "~/.codex/sessions"
codex_enabled = true
cursor = "%APPDATA%/Cursor/User/workspaceStorage"
cursor_enabled = true
aider = "~/.aider/.aider.input.history"
aider_enabled = true
zed = "~/.config/zed/conversations"
zed_enabled = true
lapce = "~/.local/share/lapce-stable/conversations"
lapce_enabled = false
backfill_on_first_run = true
backfill_days = 30
"#
        .to_string()
    }
}

fn default_port() -> u16 {
    7878
}
fn default_bind_address() -> String {
    "127.0.0.1".to_string()
}
fn default_timeout() -> u32 {
    30
}
fn default_dedup_window() -> u32 {
    60
}
fn default_database_encryption() -> String {
    "off".to_string()
}
fn default_database_key_env() -> String {
    "KODAAL_DB_KEY".to_string()
}
fn default_prompt_length() -> u32 {
    1_000_000
}
fn default_backup_count() -> u32 {
    7
}
fn default_backfill_days() -> u32 {
    30
}
fn default_suggestion_min_chars() -> u32 {
    24
}
fn default_suggestion_limit() -> u32 {
    3
}
fn default_claude_code_path() -> String {
    "~/.claude/projects".to_string()
}
fn default_codex_path() -> String {
    "~/.codex/sessions".to_string()
}
fn default_cursor_path() -> String {
    #[cfg(windows)]
    {
        "%APPDATA%/Cursor/User/workspaceStorage".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "~/Library/Application Support/Cursor/User/workspaceStorage".to_string()
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        "~/.config/Cursor/User/workspaceStorage".to_string()
    }
}
fn default_aider_path() -> String {
    "~/.aider/.aider.input.history".to_string()
}
fn default_zed_path() -> String {
    #[cfg(windows)]
    {
        "%APPDATA%/Zed/conversations".to_string()
    }
    #[cfg(not(windows))]
    {
        "~/.config/zed/conversations".to_string()
    }
}
fn default_lapce_path() -> String {
    #[cfg(windows)]
    {
        "%APPDATA%/Lapce-Stable/conversations".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "~/Library/Application Support/dev.lapce.Lapce-Stable/conversations".to_string()
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        "~/.local/share/lapce-stable/conversations".to_string()
    }
}
fn default_true() -> bool {
    true
}

fn validate_database_config(config: &DatabaseConfig) -> Result<(), String> {
    match config.encryption.as_str() {
        "off" | "sqlcipher" => {}
        _ => return Err("database.encryption must be off or sqlcipher".to_string()),
    }
    if config.key_env.trim().is_empty() || config.key_env.chars().any(char::is_whitespace) {
        return Err("database.key_env must be a non-empty environment variable name".to_string());
    }
    Ok(())
}

fn validate_prune_config(config: &PruneConfig) -> Result<(), String> {
    if matches!(config.older_than_days, Some(0)) {
        return Err("prune.older_than_days must be >= 1".to_string());
    }
    if matches!(config.shorter_than, Some(value) if value <= 0) {
        return Err("prune.shorter_than must be positive".to_string());
    }
    if let Some(source) = config.source.as_deref() {
        match source {
            "browser" | "desktop" | "ide" | "cli" | "mcp" => {}
            _ => return Err("prune.source must be browser, desktop, ide, cli, or mcp".to_string()),
        }
    }
    Ok(())
}

fn validate_suggestions_config(config: &SuggestionsConfig) -> Result<(), String> {
    if !(10..=500).contains(&config.min_chars) {
        return Err("suggestions.min_chars must be 10-500".to_string());
    }
    if !(1..=10).contains(&config.limit) {
        return Err("suggestions.limit must be 1-10".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests;

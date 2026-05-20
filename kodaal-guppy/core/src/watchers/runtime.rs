use crate::{
    app::AppState,
    db::{CapturePayload, ProjectHint},
    ids,
};
use serde_json::{Map, Value};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone)]
pub struct WatcherReport {
    pub scanned_files: usize,
    pub captured_prompts: usize,
}

#[derive(Debug, Clone)]
struct WatchTarget {
    source_app: &'static str,
    source: &'static str,
    path: PathBuf,
    kind: WatchKind,
}

#[derive(Debug, Clone, Copy)]
enum WatchKind {
    Jsonl,
    CodexJsonl,
    AiderHistory,
    ZedJson,
    LapceJson,
    CursorStateDb,
}

pub fn spawn_watcher_loop(state: AppState) {
    thread::spawn(move || loop {
        if let Err(error) = scan_once(&state) {
            eprintln!("watchers scan failed: {error}");
        }
        thread::sleep(Duration::from_secs(5));
    });
}

pub fn scan_once(state: &AppState) -> Result<WatcherReport, Box<dyn std::error::Error>> {
    if state
        .capture
        .lock()
        .map_err(|_| "capture lock poisoned")?
        .paused
    {
        return Ok(WatcherReport {
            scanned_files: 0,
            captured_prompts: 0,
        });
    }

    let mut report = WatcherReport {
        scanned_files: 0,
        captured_prompts: 0,
    };
    for target in watch_targets(state) {
        if target.path.is_dir() {
            for file in candidate_files(&target.path, target.kind) {
                report.scanned_files += 1;
                report.captured_prompts += scan_file(state, &target, &file)?;
            }
        } else if target.path.is_file() {
            report.scanned_files += 1;
            report.captured_prompts += scan_file(state, &target, &target.path)?;
        }
    }
    Ok(report)
}

fn watch_targets(state: &AppState) -> Vec<WatchTarget> {
    let config = &state.config.watchers;
    let mut targets = Vec::new();
    if config.claude_code_enabled {
        targets.push(WatchTarget {
            source_app: "claude-code",
            source: "cli",
            path: expand_path(&config.claude_code),
            kind: WatchKind::Jsonl,
        });
    }
    if config.codex_enabled {
        targets.push(WatchTarget {
            source_app: "codex",
            source: "cli",
            path: expand_path(&config.codex),
            kind: WatchKind::CodexJsonl,
        });
    }
    if config.cursor_enabled {
        targets.push(WatchTarget {
            source_app: "cursor",
            source: "ide",
            path: expand_path(&config.cursor),
            kind: WatchKind::CursorStateDb,
        });
    }
    if config.aider_enabled {
        targets.push(WatchTarget {
            source_app: "aider",
            source: "cli",
            path: expand_path(&config.aider),
            kind: WatchKind::AiderHistory,
        });
    }
    if config.zed_enabled {
        targets.push(WatchTarget {
            source_app: "zed",
            source: "ide",
            path: expand_path(&config.zed),
            kind: WatchKind::ZedJson,
        });
    }
    if config.lapce_enabled {
        targets.push(WatchTarget {
            source_app: "lapce",
            source: "ide",
            path: expand_path(&config.lapce),
            kind: WatchKind::LapceJson,
        });
    }
    targets
}

fn scan_file(
    state: &AppState,
    target: &WatchTarget,
    file: &Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let path_key = file.to_string_lossy().to_string();
    if blocklisted_path(state, &path_key)? {
        return Ok(0);
    }
    if matches!(target.kind, WatchKind::CursorStateDb) {
        return scan_cursor_state_db(state, target, file, &path_key);
    }
    let file_len = fs::metadata(file)?.len();
    let offset = state
        .db
        .lock()
        .map_err(|_| "db lock poisoned")?
        .watcher_offset(target.source_app, &path_key)?;
    if offset == 0 && skip_initial_backfill(state, &fs::metadata(file)?) {
        state
            .db
            .lock()
            .map_err(|_| "db lock poisoned")?
            .set_watcher_offset(target.source_app, &path_key, file_len)?;
        return Ok(0);
    }
    if offset >= file_len {
        return Ok(0);
    }
    let text = read_from_offset(file, offset)?;
    let parsed = match target.kind {
        WatchKind::Jsonl => parse_jsonl(&text, &path_key, target.source, target.source_app),
        WatchKind::CodexJsonl => parse_codex_jsonl(&text, &path_key),
        WatchKind::AiderHistory => parse_aider_history(&text, &path_key),
        WatchKind::ZedJson => parse_zed_json(&text, &path_key),
        WatchKind::LapceJson => parse_lapce_json(&text, &path_key),
        WatchKind::CursorStateDb => Vec::new(),
    };
    let mut captured = 0;
    for payload in parsed {
        if blocklisted_payload(state, &payload)? {
            continue;
        }
        let text_hash = ids::sha256_hex(&payload.text);
        let response = state
            .db
            .lock()
            .map_err(|_| "db lock poisoned")?
            .ingest_prompt(payload.clone())?;
        captured += 1;
        append_audit(
            state,
            format!(
                "watcher_capture prompt_id={} source={} source_app={} hash={} deduped={}",
                response.id, payload.source, payload.source_app, text_hash, response.deduped
            ),
        )?;
    }
    state
        .db
        .lock()
        .map_err(|_| "db lock poisoned")?
        .set_watcher_offset(target.source_app, &path_key, file_len)?;
    Ok(captured)
}

fn scan_cursor_state_db(
    state: &AppState,
    target: &WatchTarget,
    file: &Path,
    path_key: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let offset = state
        .db
        .lock()
        .map_err(|_| "db lock poisoned")?
        .watcher_offset(target.source_app, path_key)?;
    let parsed = parse_cursor_state_db(file, path_key, offset)?;
    if offset == 0 && skip_initial_backfill(state, &fs::metadata(file)?) {
        state
            .db
            .lock()
            .map_err(|_| "db lock poisoned")?
            .set_watcher_offset(target.source_app, path_key, parsed.next_offset)?;
        return Ok(0);
    }
    let mut captured = 0;
    for payload in parsed.prompts {
        if blocklisted_payload(state, &payload)? {
            continue;
        }
        let text_hash = ids::sha256_hex(&payload.text);
        let response = state
            .db
            .lock()
            .map_err(|_| "db lock poisoned")?
            .ingest_prompt(payload.clone())?;
        captured += 1;
        append_audit(
            state,
            format!(
                "watcher_capture prompt_id={} source={} source_app={} hash={} deduped={}",
                response.id, payload.source, payload.source_app, text_hash, response.deduped
            ),
        )?;
    }
    state
        .db
        .lock()
        .map_err(|_| "db lock poisoned")?
        .set_watcher_offset(target.source_app, path_key, parsed.next_offset)?;
    Ok(captured)
}

fn skip_initial_backfill(state: &AppState, metadata: &fs::Metadata) -> bool {
    let config = &state.config.watchers;
    if !config.backfill_on_first_run {
        return true;
    }
    if config.backfill_days == 0 {
        return true;
    }
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    let Some(window_start) =
        SystemTime::now().checked_sub(Duration::from_secs(config.backfill_days as u64 * 86_400))
    else {
        return true;
    };
    modified < window_start
}

fn read_from_offset(path: &Path, offset: u64) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

fn candidate_files(root: &Path, kind: WatchKind) -> Vec<PathBuf> {
    let mut output = Vec::new();
    collect_files(root, kind, &mut output);
    output
}

fn collect_files(root: &Path, kind: WatchKind, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "node_modules" || name == "target" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, kind, output);
        } else if matches_kind(&path, kind) {
            output.push(path);
        }
    }
}

fn matches_kind(path: &Path, kind: WatchKind) -> bool {
    match kind {
        WatchKind::Jsonl | WatchKind::CodexJsonl => {
            path.extension().is_some_and(|ext| ext == "jsonl")
        }
        WatchKind::AiderHistory => true,
        WatchKind::ZedJson | WatchKind::LapceJson => {
            path.extension().is_some_and(|ext| ext == "json")
        }
        WatchKind::CursorStateDb => path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("state.vscdb")),
    }
}

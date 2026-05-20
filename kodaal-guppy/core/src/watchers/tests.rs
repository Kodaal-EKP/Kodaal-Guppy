#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PromptQuery;
    use serde_json::json;
    use std::io::Write;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn test_fr020_jsonl_parses_only_user_turns() {
        let lines = [
            json!({"type":"user","message":{"content":[{"type":"text","text":"capture this"}]},"cwd":"/work/api","session_id":"s1"}).to_string(),
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"ignore assistant"}]}}).to_string(),
            json!({"type":"tool_result","content":"ignore tool"}).to_string(),
            json!({"role":"user","content":[{"type":"tool_result","text":"ignore nested tool"},{"type":"text","text":"second prompt"}],"sessionId":"s2"}).to_string(),
        ]
        .join("\n");
        let prompts = parse_jsonl(&lines, "/tmp/session.jsonl", "cli", "codex");
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0].text, "capture this");
        assert_eq!(prompts[0].source_app, "codex");
        assert_eq!(prompts[0].project_hint.as_ref().unwrap().value, "/work/api");
        assert_eq!(prompts[1].text, "second prompt");
    }

    #[test]
    fn test_fr016_codex_desktop_payload_is_classified_as_desktop() {
        let lines = [
            json!({"type":"session_meta","payload":{"id":"codex-session","cwd":"C:/work/kodaal","source":"vscode","originator":"Codex Desktop"}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"codex desktop prompt"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ignore assistant"}]}}).to_string(),
        ]
        .join("\n");
        let prompts = parse_codex_jsonl(&lines, "rollout.jsonl");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].text, "codex desktop prompt");
        assert_eq!(prompts[0].source, "desktop");
        assert_eq!(prompts[0].source_app, "codex-desktop");
        assert_eq!(prompts[0].conversation_id.as_deref(), Some("codex-session"));
        assert_eq!(prompts[0].project_hint.as_ref().unwrap().value, "C:/work/kodaal");
    }

    #[test]
    fn test_fr016_codex_vscode_payload_is_classified_as_ide() {
        let lines = [
            json!({"type":"session_meta","payload":{"id":"codex-session","cwd":"C:/work/kodaal","source":"vscode","originator":"codex_vscode"}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"codex ide prompt"}]}}).to_string(),
        ]
        .join("\n");
        let prompts = parse_codex_jsonl(&lines, "rollout.jsonl");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].source, "ide");
        assert_eq!(prompts[0].source_app, "codex-vscode");
    }

    #[test]
    fn test_fr020_internal_context_notifications_are_not_captured_as_prompts() {
        let lines = [
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for C:/work\n\n<INSTRUCTIONS>ignore</INSTRUCTIONS>"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>C:/work</cwd>\n</environment_context>"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Tool loaded."}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<<autonomous-loop-dynamic>>"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.\n\nSummary:\nignore"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"The following is the Codex agent history added since your last approval assessment. Continue assessing the request action."}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<command-message>loop</command-message>\n<command-name>/loop</command-name>\n<command-args>build the app</command-args>"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# Files mentioned by the user:\n\n## a.md: C:/a.md\n\n## My request for Codex:\nfix the parser"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# /loop — schedule a recurring prompt\n\ninternal command docs\n\n## Input\n\nbuild the parser"}]}}).to_string(),
        ]
        .join("\n");
        let prompts = parse_codex_jsonl(&lines, "rollout.jsonl");
        assert_eq!(prompts.len(), 3);
        assert_eq!(prompts[0].text, "/loop build the app");
        assert_eq!(prompts[1].text, "fix the parser");
        assert_eq!(prompts[2].text, "/loop build the parser");
    }

    #[test]
    fn test_fr015_claude_vscode_entrypoint_is_classified_as_ide() {
        let lines = [
            json!({"type":"user","entrypoint":"claude-vscode","message":{"role":"user","content":"claude extension prompt"},"isSidechain":false,"sessionId":"s1"}).to_string(),
            json!({"type":"user","entrypoint":"claude-vscode","message":{"role":"user","content":[{"type":"tool_result","content":"command output is not a prompt"}]},"isSidechain":false}).to_string(),
            json!({"type":"user","entrypoint":"claude-vscode","message":{"role":"user","content":"sidechain instruction"},"isSidechain":true}).to_string(),
            json!({"type":"user","entrypoint":"claude-vscode","message":{"role":"user","content":"compact summary"},"isCompactSummary":true,"isVisibleInTranscriptOnly":true}).to_string(),
        ]
        .join("\n");
        let prompts = parse_jsonl(&lines, "claude-session.jsonl", "cli", "claude-code");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].text, "claude extension prompt");
        assert_eq!(prompts[0].source, "ide");
        assert_eq!(prompts[0].source_app, "claude-vscode");
        assert_eq!(
            prompts[0]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("entrypoint"))
                .and_then(Value::as_str),
            Some("claude-vscode")
        );
    }

    #[test]
    fn test_fr015_claude_code_without_entrypoint_stays_cli() {
        let lines = json!({"type":"user","message":{"role":"user","content":"plain claude code prompt"},"sessionId":"s1"}).to_string();
        let prompts = parse_jsonl(&lines, "claude-session.jsonl", "cli", "claude-code");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].source, "cli");
        assert_eq!(prompts[0].source_app, "claude-code");
    }

    #[test]
    fn test_fr011_cursor_state_db_parser_reads_ai_service_prompts() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(
            dir.path().join("workspace.json"),
            r#"{"folder":"file:///C%3A/work/cursor-project"}"#,
        )
        .expect("workspace json");
        let db_path = dir.path().join("state.vscdb");
        let conn = rusqlite::Connection::open(&db_path).expect("cursor db");
        conn.execute("CREATE TABLE ItemTable (key TEXT, value BLOB)", [])
            .expect("table");
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('aiService.prompts', ?1)",
            [json!([
                {"text":"first old prompt","commandType":"edit"},
                {"text":"new cursor prompt","commandType":"chat"}
            ])
            .to_string()],
        )
        .expect("insert");
        drop(conn);

        let parsed =
            parse_cursor_state_db(&db_path, "state.vscdb", 1).expect("parse cursor state");
        assert_eq!(parsed.next_offset, 2);
        assert_eq!(parsed.prompts.len(), 1);
        assert_eq!(parsed.prompts[0].text, "new cursor prompt");
        assert_eq!(parsed.prompts[0].source, "ide");
        assert_eq!(parsed.prompts[0].source_app, "cursor");
        let expected_path = if cfg!(windows) {
            "C:\\work\\cursor-project"
        } else {
            "/C:/work/cursor-project"
        };
        assert_eq!(parsed.prompts[0].project_hint.as_ref().unwrap().value, expected_path);
    }

    #[test]
    fn test_fr017_aider_history_splits_prompt_blocks() {
        let history = "# 2026-05-05 10:11:12.000000\nfirst prompt\nline two\n# 2026-05-05 10:12:12.000000\nsecond prompt\n";
        let prompts = parse_aider_history(history, "/tmp/.aider.input.history");
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0].text, "first prompt\nline two");
        assert_eq!(prompts[1].source_app, "aider");
    }

    #[test]
    fn test_fr012_zed_parser_uses_worktree_project_hint() {
        let text = json!({
            "id": "zed-1",
            "title": "schema work",
            "worktree_paths": ["C:/work/project"],
            "messages": [
                {"role": "assistant", "content": "ignored"},
                {"role": "user", "content": [{"type": "text", "text": "zed prompt"}]}
            ]
        })
        .to_string();
        let prompts = parse_zed_json(&text, "conversation.json");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].text, "zed prompt");
        assert_eq!(prompts[0].source, "ide");
        assert_eq!(prompts[0].project_hint.as_ref().unwrap().kind, "path");
    }

    #[test]
    fn test_fr013_lapce_parser_uses_workspace_project_hint() {
        let text = json!({
            "session_id": "lapce-1",
            "summary": "refactor",
            "workspace_paths": ["C:/work/lapce-project"],
            "conversation": {
                "messages": [
                    {"role": "assistant", "content": "ignored"},
                    {"role": "user", "content": "lapce prompt"}
                ]
            }
        })
        .to_string();
        let prompts = parse_lapce_json(&text, "lapce-conversation.json");
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].text, "lapce prompt");
        assert_eq!(prompts[0].source, "ide");
        assert_eq!(prompts[0].source_app, "lapce");
        assert_eq!(prompts[0].project_hint.as_ref().unwrap().kind, "path");
    }

    #[test]
    fn test_fr015_first_run_without_backfill_sets_offset_then_captures_appended_turn() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().expect("temp dir");
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).expect("sessions dir");
        let log = sessions.join("session.jsonl");
        fs::write(
            &log,
            format!(
                "{}\n",
                json!({"type":"user","message":{"content":[{"type":"text","text":"existing prompt"}]},"cwd":"/work/one"})
            ),
        )
        .expect("initial log");
        write_config(
            dir.path(),
            &format!(
                r#"[watchers]
claude_code = "{}"
claude_code_enabled = true
codex_enabled = false
cursor_enabled = false
aider_enabled = false
zed_enabled = false
backfill_on_first_run = false
backfill_days = 30
"#,
                toml_path(&sessions)
            ),
        );
        std::env::set_var("KODAAL_HOME", dir.path());
        let state = AppState::load().expect("state");

        let report = scan_once(&state).expect("first scan");
        assert_eq!(report.scanned_files, 1);
        assert_eq!(report.captured_prompts, 0);

        fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .expect("append log")
            .write_all(
                format!(
                    "{}\n",
                    json!({"type":"user","message":{"content":[{"type":"text","text":"new appended prompt"}]},"cwd":"/work/two"})
                )
                .as_bytes(),
            )
            .expect("append prompt");
        let report = scan_once(&state).expect("second scan");
        assert_eq!(report.captured_prompts, 1);
        let prompts = state
            .db
            .lock()
            .expect("db")
            .list_prompts(PromptQuery::default())
            .expect("prompts");
        assert_eq!(prompts.total, 1);
        assert_eq!(prompts.items[0].text, "new appended prompt");
    }

    #[test]
    fn test_fr015_first_run_backfills_recent_file_and_skips_outside_window() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().expect("temp dir");
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).expect("sessions dir");
        let log = sessions.join("session.jsonl");
        fs::write(
            &log,
            format!(
                "{}\n",
                json!({"type":"user","message":{"content":[{"type":"text","text":"recent backfill prompt"}]},"cwd":"/work/recent"})
            ),
        )
        .expect("recent log");
        write_config(
            dir.path(),
            &format!(
                r#"[watchers]
claude_code = "{}"
claude_code_enabled = true
codex_enabled = false
cursor_enabled = false
aider_enabled = false
zed_enabled = false
backfill_on_first_run = true
backfill_days = 30
"#,
                toml_path(&sessions)
            ),
        );
        std::env::set_var("KODAAL_HOME", dir.path());
        let state = AppState::load().expect("state");
        assert_eq!(scan_once(&state).expect("scan").captured_prompts, 1);

        let dir = tempfile::tempdir().expect("temp dir");
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).expect("sessions dir");
        fs::write(
            sessions.join("session.jsonl"),
            format!(
                "{}\n",
                json!({"type":"user","message":{"content":[{"type":"text","text":"outside window"}]},"cwd":"/work/old"})
            ),
        )
        .expect("old log");
        write_config(
            dir.path(),
            &format!(
                r#"[watchers]
claude_code = "{}"
claude_code_enabled = true
codex_enabled = false
cursor_enabled = false
aider_enabled = false
zed_enabled = false
backfill_on_first_run = true
backfill_days = 0
"#,
                toml_path(&sessions)
            ),
        );
        std::env::set_var("KODAAL_HOME", dir.path());
        let state = AppState::load().expect("state");
        assert_eq!(scan_once(&state).expect("scan").captured_prompts, 0);
    }

    #[test]
    fn test_fr020_watcher_respects_pause_and_blocklisted_paths() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().expect("temp dir");
        let sessions = dir.path().join("private-sessions");
        fs::create_dir_all(&sessions).expect("sessions dir");
        fs::write(
            sessions.join("session.jsonl"),
            format!(
                "{}\n",
                json!({"type":"user","message":{"content":[{"type":"text","text":"blocked prompt"}]},"cwd":"/work/private"})
            ),
        )
        .expect("log");
        write_config(
            dir.path(),
            &format!(
                r#"[capture]
paused = true

[capture.blocklist]
paths = ["private-sessions"]

[watchers]
claude_code = "{}"
claude_code_enabled = true
codex_enabled = false
cursor_enabled = false
aider_enabled = false
zed_enabled = false
backfill_on_first_run = true
backfill_days = 30
"#,
                toml_path(&sessions)
            ),
        );
        std::env::set_var("KODAAL_HOME", dir.path());
        let state = AppState::load().expect("state");
        assert_eq!(scan_once(&state).expect("paused scan").scanned_files, 0);
        state.capture.lock().expect("capture").paused = false;
        let report = scan_once(&state).expect("blocklisted scan");
        assert_eq!(report.scanned_files, 1);
        assert_eq!(report.captured_prompts, 0);
    }

    fn write_config(home: &Path, text: &str) {
        fs::write(home.join("config.toml"), text).expect("config");
    }

    fn toml_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }
}

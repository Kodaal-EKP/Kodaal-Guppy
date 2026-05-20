pub fn parse_jsonl(
    text: &str,
    path_key: &str,
    source: &str,
    source_app: &str,
) -> Vec<CapturePayload> {
    text.lines()
        .filter_map(|line| {
            let value = serde_json::from_str::<Value>(line).ok()?;
            parse_user_json_event(&value, path_key, source, source_app)
        })
        .collect()
}

pub fn parse_codex_jsonl(text: &str, path_key: &str) -> Vec<CapturePayload> {
    let mut context = CodexContext::default();
    let mut prompts = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            context.apply_session_meta(&value);
            continue;
        }
        if let Some(payload) = parse_codex_user_event(&value, path_key, &context) {
            prompts.push(payload);
        }
    }
    prompts
}

#[derive(Default)]
struct CodexContext {
    session_id: Option<String>,
    cwd: Option<String>,
    host_source: Option<String>,
    originator: Option<String>,
}

impl CodexContext {
    fn apply_session_meta(&mut self, value: &Value) {
        self.session_id = string_at(value, &["/payload/id"]).or_else(|| self.session_id.clone());
        self.cwd = string_at(value, &["/payload/cwd"]).or_else(|| self.cwd.clone());
        self.host_source = string_at(value, &["/payload/source"]).or_else(|| self.host_source.clone());
        self.originator =
            string_at(value, &["/payload/originator"]).or_else(|| self.originator.clone());
    }

    fn source(&self) -> String {
        if matches!(self.originator.as_deref(), Some("Codex Desktop")) {
            return "desktop".to_string();
        }
        match self.host_source.as_deref() {
            Some("vscode" | "cursor" | "ide") => "ide".to_string(),
            Some("exec") => "cli".to_string(),
            _ => "cli".to_string(),
        }
    }

    fn source_app(&self) -> String {
        if matches!(self.originator.as_deref(), Some("Codex Desktop")) {
            return "codex-desktop".to_string();
        }
        match self.host_source.as_deref() {
            Some("vscode") => "codex-vscode".to_string(),
            Some("cursor") => "codex-cursor".to_string(),
            Some("ide") => "codex-ide".to_string(),
            Some("exec") => "codex-cli".to_string(),
            _ => "codex-cli".to_string(),
        }
    }
}

fn parse_codex_user_event(
    value: &Value,
    path_key: &str,
    context: &CodexContext,
) -> Option<CapturePayload> {
    if !is_user_event(value) {
        return None;
    }
    let text = normalize_prompt_text(&extract_text(value)?)?;
    let cwd = string_at(value, &["/cwd", "/message/cwd", "/payload/cwd"])
        .or_else(|| context.cwd.clone());
    let session_id = string_at(
        value,
        &[
            "/session_id",
            "/sessionId",
            "/conversation_id",
            "/message/session_id",
            "/payload/session_id",
        ],
    )
    .or_else(|| context.session_id.clone());
    Some(CapturePayload {
        text,
        source: context.source(),
        source_app: context.source_app(),
        project_hint: cwd.map(|value| ProjectHint {
            kind: "cwd".to_string(),
            value,
        }),
        conversation_id: session_id.clone(),
        conversation_title: None,
        metadata: Some(metadata([
            ("source_file", Some(path_key.to_string())),
            ("session_id", session_id),
            ("originator", context.originator.clone()),
            ("host_source", context.host_source.clone()),
        ])),
    })
}

pub struct CursorParseResult {
    pub prompts: Vec<CapturePayload>,
    pub next_offset: u64,
}

pub fn parse_cursor_state_db(
    path: &Path,
    path_key: &str,
    start_index: u64,
) -> Result<CursorParseResult, Box<dyn std::error::Error>> {
    let connection = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'aiService.prompts'",
            [],
            |row| match row.get_ref(0)? {
                rusqlite::types::ValueRef::Text(value) => {
                    Ok(String::from_utf8_lossy(value).to_string())
                }
                rusqlite::types::ValueRef::Blob(value) => {
                    Ok(String::from_utf8_lossy(value).to_string())
                }
                _ => Ok(String::new()),
            },
        )
        .ok();
    let Some(value) = value else {
        return Ok(CursorParseResult {
            prompts: Vec::new(),
            next_offset: 0,
        });
    };
    let items = serde_json::from_str::<Value>(&value)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let project_hint = cursor_workspace_path(path).map(|value| ProjectHint {
        kind: "path".to_string(),
        value,
    });
    let conversation_id = path
        .parent()
        .and_then(|value| value.file_name())
        .map(|value| format!("cursor:{}", value.to_string_lossy()));
    let mut prompts = Vec::new();
    for (index, item) in items.iter().enumerate().skip(start_index as usize) {
        let Some(text) = item.get("text").and_then(Value::as_str) else {
            continue;
        };
        let Some(text) = normalize_prompt_text(text) else {
            continue;
        };
        let command_type = item
            .get("commandType")
            .and_then(Value::as_str)
            .map(str::to_string);
        prompts.push(CapturePayload {
            text,
            source: "ide".to_string(),
            source_app: "cursor".to_string(),
            project_hint: project_hint.clone(),
            conversation_id: conversation_id.clone(),
            conversation_title: None,
            metadata: Some(metadata([
                ("source_file", Some(path_key.to_string())),
                ("cursor_key", Some("aiService.prompts".to_string())),
                ("cursor_index", Some(index.to_string())),
                ("command_type", command_type),
            ])),
        });
    }
    Ok(CursorParseResult {
        prompts,
        next_offset: items.len() as u64,
    })
}

fn cursor_workspace_path(path: &Path) -> Option<String> {
    let workspace_json = path.parent()?.join("workspace.json");
    let text = fs::read_to_string(workspace_json).ok()?;
    let value = serde_json::from_str::<Value>(&text).ok()?;
    let folder = value.get("folder")?.as_str()?;
    file_uri_to_path(folder)
}

fn file_uri_to_path(value: &str) -> Option<String> {
    let rest = value.strip_prefix("file:///")?;
    let decoded = percent_decode(rest);
    #[cfg(windows)]
    {
        Some(decoded.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        Some(format!("/{decoded}"))
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[index + 1..index + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

fn parse_user_json_event(
    value: &Value,
    path_key: &str,
    source: &str,
    source_app: &str,
) -> Option<CapturePayload> {
    if !is_user_event(value) {
        return None;
    }
    let text = normalize_prompt_text(&extract_text(value)?)?;
    let attribution = resolve_generic_attribution(value, source, source_app);
    let cwd = string_at(value, &["/cwd", "/message/cwd", "/payload/cwd"]);
    let session_id = string_at(
        value,
        &[
            "/session_id",
            "/sessionId",
            "/conversation_id",
            "/message/session_id",
            "/payload/session_id",
        ],
    );
    Some(CapturePayload {
        text,
        source: attribution.source,
        source_app: attribution.source_app,
        project_hint: cwd.clone().map(|value| ProjectHint {
            kind: "cwd".to_string(),
            value,
        }),
        conversation_id: session_id.clone(),
        conversation_title: None,
        metadata: Some(metadata([
            ("source_file", Some(path_key.to_string())),
            ("session_id", session_id),
            ("entrypoint", attribution.entrypoint),
        ])),
    })
}

pub fn parse_aider_history(text: &str, path_key: &str) -> Vec<CapturePayload> {
    let mut prompts = Vec::new();
    let mut current = Vec::<String>::new();
    for line in text.lines() {
        if is_aider_header(line) {
            push_aider_prompt(&mut prompts, &mut current, path_key);
        } else {
            current.push(line.to_string());
        }
    }
    push_aider_prompt(&mut prompts, &mut current, path_key);
    prompts
}

fn push_aider_prompt(prompts: &mut Vec<CapturePayload>, current: &mut Vec<String>, path_key: &str) {
    let Some(text) = normalize_prompt_text(&current.join("\n")) else {
        current.clear();
        return;
    };
    current.clear();
    prompts.push(CapturePayload {
        text,
        source: "cli".to_string(),
        source_app: "aider".to_string(),
        project_hint: None,
        conversation_id: None,
        conversation_title: None,
        metadata: Some(metadata([("source_file", Some(path_key.to_string()))])),
    });
}

fn is_aider_header(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("# ") else {
        return false;
    };
    rest.len() >= 19
        && rest.as_bytes().get(4) == Some(&b'-')
        && rest.as_bytes().get(7) == Some(&b'-')
        && rest.as_bytes().get(10) == Some(&b' ')
        && rest.as_bytes().get(13) == Some(&b':')
}

pub fn parse_zed_json(text: &str, path_key: &str) -> Vec<CapturePayload> {
    parse_ide_json_conversation(text, path_key, "zed")
}

pub fn parse_lapce_json(text: &str, path_key: &str) -> Vec<CapturePayload> {
    parse_ide_json_conversation(text, path_key, "lapce")
}

fn parse_ide_json_conversation(
    text: &str,
    path_key: &str,
    source_app: &str,
) -> Vec<CapturePayload> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let project_path = string_at(
        &value,
        &[
            "/worktree_paths/0",
            "/workspace_paths/0",
            "/workspace",
            "/project_path",
        ],
    );
    let conversation_id = string_at(&value, &["/id", "/conversation_id", "/session_id"]);
    let conversation_title = string_at(&value, &["/title", "/summary"]);
    let mut output = Vec::new();
    for message in messages_array(&value) {
        if !is_user_event(message) {
            continue;
        }
        let Some(text) = extract_text(message).and_then(|text| normalize_prompt_text(&text)) else {
            continue;
        };
        output.push(CapturePayload {
            text,
            source: "ide".to_string(),
            source_app: source_app.to_string(),
            project_hint: project_path.clone().map(|value| ProjectHint {
                kind: "path".to_string(),
                value,
            }),
            conversation_id: conversation_id.clone(),
            conversation_title: conversation_title.clone(),
            metadata: Some(metadata([("source_file", Some(path_key.to_string()))])),
        });
    }
    output
}

fn messages_array(value: &Value) -> Vec<&Value> {
    ["/messages", "/conversation/messages", "/items"]
        .iter()
        .find_map(|path| value.pointer(path).and_then(Value::as_array))
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn is_user_event(value: &Value) -> bool {
    if bool_at(
        value,
        &[
            "/isSidechain",
            "/isCompactSummary",
            "/isVisibleInTranscriptOnly",
            "/message/isSidechain",
            "/payload/isSidechain",
        ],
    ) {
        return false;
    }
    [
        "/role",
        "/message/role",
        "/payload/role",
        "/type",
        "/message/type",
        "/payload/type",
    ]
    .iter()
    .filter_map(|path| value.pointer(path).and_then(Value::as_str))
        .any(|role| role == "user" || role == "human")
}

fn extract_text(value: &Value) -> Option<String> {
    for path in [
        "/text",
        "/content",
        "/message/text",
        "/message/content",
        "/payload/text",
        "/payload/content",
    ] {
        if let Some(text) = value.pointer(path).and_then(Value::as_str) {
            return Some(text.to_string());
        }
        if let Some(array) = value.pointer(path).and_then(Value::as_array) {
            let joined = array
                .iter()
                .filter_map(extract_content_item_text)
                .collect::<Vec<_>>()
                .join("\n");
            if !joined.trim().is_empty() {
                return Some(joined);
            }
        }
    }
    None
}

fn extract_content_item_text(value: &Value) -> Option<String> {
    if value.as_str().is_some() {
        return value.as_str().map(str::to_string);
    }
    if matches!(
        value.get("type").and_then(Value::as_str),
        Some("tool_result" | "tool_use" | "image")
    ) {
        return None;
    }
    value
        .get("text")
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn string_at(value: &Value, paths: &[&str]) -> Option<String> {
    paths
        .iter()
        .find_map(|path| value.pointer(path).and_then(Value::as_str))
        .map(str::to_string)
}

fn metadata<const N: usize>(entries: [(&str, Option<String>); N]) -> Map<String, Value> {
    let mut map = Map::new();
    for (key, value) in entries {
        if let Some(value) = value {
            map.insert(key.to_string(), Value::String(value));
        }
    }
    map
}

async fn create_prompt(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<(StatusCode, Json<crate::db::CaptureResponse>), ApiError> {
    if state
        .capture
        .lock()
        .map_err(|_| ApiError::internal("capture lock poisoned"))?
        .paused
    {
        return Err(ApiError::conflict("CAPTURE_PAUSED", "capture is paused"));
    }
    let payload: CapturePayload = parse_json(&body)?;
    if is_blocklisted(&state, &payload) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "capture blocked by privacy blocklist",
            None,
        ));
    }
    let source = payload.source.clone();
    let source_app = payload.source_app.clone();
    let text_hash = crate::ids::sha256_hex(&payload.text);
    let response = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .ingest_prompt(payload)?;
    append_audit(
        &state,
        format!(
            "capture prompt_id={} source={} source_app={} hash={} deduped={}",
            response.id, source, source_app, text_hash, response.deduped
        ),
    )?;
    if !response.deduped {
        arm_artifact_watcher(&state, response.id.clone(), response.project_id.clone());
    }
    Ok((StatusCode::CREATED, Json(response)))
}

const ARTIFACT_WATCH_WINDOW: StdDuration = StdDuration::from_secs(120);
const ARTIFACT_WATCH_POLL: StdDuration = StdDuration::from_millis(250);
const DEFAULT_IGNORED_ARTIFACT_DIRS: [&str; 7] = [
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "target",
    "dist",
    "build",
];
const DEFAULT_IGNORED_ARTIFACT_PATTERNS: [&str; 10] = [
    ".DS_Store",
    ".gitignore",
    "*.bin",
    "*.log",
    "*.tmp",
    ".cache/",
    ".next/",
    ".nuxt/",
    "__pycache__/",
    "*.pyc",
];

fn arm_artifact_watcher(state: &AppState, prompt_id: String, project_id: Option<String>) {
    let Some(project_id) = project_id else {
        return;
    };
    let state = state.clone();
    thread::spawn(move || {
        let Ok(project) = state.db.lock().map_err(|_| ()).and_then(|db| {
            db.get_project(&project_id)
                .map_err(|_| ())
        }) else {
            return;
        };
        let Some(project_path) = project.path else {
            return;
        };
        if project_path.starts_with("domain://") {
            return;
        }
        let root = FsPathBuf::from(project_path);
        if !root.is_dir() {
            return;
        }

        let started_at = SystemTime::now();
        let deadline = Instant::now() + ARTIFACT_WATCH_WINDOW;
        let ignore_patterns = gitignore_patterns(&root);
        let mut seen = HashSet::new();

        while Instant::now() < deadline {
            if !root.is_dir() {
                break;
            }
            for path in new_artifact_candidates(&root, started_at, &ignore_patterns) {
                if !seen.insert(path.clone()) {
                    continue;
                }
                let Ok(mut db) = state.db.lock() else {
                    continue;
                };
                let Ok(Some(artifact)) = db.link_auto_artifact(&prompt_id, &path) else {
                    continue;
                };
                drop(db);
                let _ = append_audit(
                    &state,
                    format!(
                        "artifact_auto_attach artifact_id={} prompt_id={} path_hash={}",
                        artifact.id,
                        prompt_id,
                        crate::ids::sha256_hex(&artifact.original_path)
                    ),
                );
            }
            thread::sleep(ARTIFACT_WATCH_POLL);
        }
    });
}

fn new_artifact_candidates(
    root: &FsPath,
    started_at: SystemTime,
    ignore_patterns: &[String],
) -> Vec<FsPathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut candidates = Vec::new();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if artifact_path_is_ignored(root, &path, ignore_patterns) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            if modified.duration_since(started_at).is_ok() {
                candidates.push(path);
            }
        }
    }
    candidates
}

fn artifact_path_is_ignored(root: &FsPath, path: &FsPath, patterns: &[String]) -> bool {
    if path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        DEFAULT_IGNORED_ARTIFACT_DIRS.contains(&name.as_ref())
    }) {
        return true;
    }
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_text = relative.to_string_lossy().replace('\\', "/");
    for pattern in DEFAULT_IGNORED_ARTIFACT_PATTERNS
        .iter()
        .copied()
        .chain(patterns.iter().map(String::as_str))
    {
        if pattern_matches_artifact_path(pattern, relative, &relative_text) {
            return true;
        }
    }
    false
}

fn pattern_matches_artifact_path(pattern: &str, relative: &FsPath, relative_text: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.starts_with('#') || pattern.starts_with('!') {
        return false;
    }
    let pattern = pattern.trim_start_matches('/').replace('\\', "/");
    if let Some(dir) = pattern.strip_suffix('/') {
        return relative_text == dir || relative_text.starts_with(&format!("{dir}/"));
    }
    if let Some(extension) = pattern.strip_prefix("*.") {
        return relative
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == extension);
    }
    if pattern.contains('/') {
        return relative_text == pattern || relative_text.starts_with(&format!("{pattern}/"));
    }
    relative.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name == pattern)
    })
}

fn gitignore_patterns(root: &FsPath) -> Vec<String> {
    std::fs::read_to_string(root.join(".gitignore"))
        .map(|value| value.lines().map(str::trim).map(ToString::to_string).collect())
        .unwrap_or_default()
}

async fn list_prompts(
    State(state): State<AppState>,
    Query(query): Query<PromptQuery>,
) -> Result<Json<crate::db::PromptList>, ApiError> {
    let response = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .list_prompts(query)?;
    Ok(Json(response))
}

async fn suggest_prompts(
    State(state): State<AppState>,
    Query(query): Query<SuggestionQuery>,
) -> Result<Json<crate::db::PromptSuggestionList>, ApiError> {
    let suggestions = state
        .capture
        .lock()
        .map_err(|_| ApiError::internal("capture lock poisoned"))?
        .suggestions
        .clone();
    let response = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .suggest_prompts(query, &suggestions)?;
    Ok(Json(response))
}

async fn get_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::db::Prompt>, ApiError> {
    Ok(Json(
        state
            .db
            .lock()
            .map_err(|_| ApiError::internal("db lock poisoned"))?
            .get_prompt(&id)?,
    ))
}

async fn update_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<crate::db::Prompt>, ApiError> {
    let update: UpdatePrompt = parse_json(&body)?;
    let favorite_changed = update.favorite.is_some();
    let title_changed = update.conversation_title.is_some();
    let prompt = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .update_prompt(&id, update)?;
    append_audit(
        &state,
        format!(
            "prompt_update prompt_id={} favorite_changed={} conversation_title_changed={}",
            id, favorite_changed, title_changed
        ),
    )?;
    Ok(Json(prompt))
}

async fn delete_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .delete_prompt(&id)?;
    append_audit(&state, format!("prompt_delete prompt_id={}", id))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct ReuseResponse {
    use_count: i64,
    last_used_at: String,
}

async fn reuse_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReuseResponse>, ApiError> {
    let (use_count, last_used_at) = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .reuse_prompt(&id)?;
    append_audit(
        &state,
        format!("prompt_reuse prompt_id={} use_count={}", id, use_count),
    )?;
    Ok(Json(ReuseResponse {
        use_count,
        last_used_at,
    }))
}

async fn prune_prompts(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<crate::db::PruneResponse>, ApiError> {
    let request: PruneRequest = parse_json(&body)?;
    let response = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .prune(request)?;
    append_audit(
        &state,
        format!(
            "prune deleted={} dry_run={}",
            response.deleted, response.dry_run
        ),
    )?;
    Ok(Json(response))
}

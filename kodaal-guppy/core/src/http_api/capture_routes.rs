#[derive(serde::Deserialize)]
struct PauseRequest {
    reason: Option<String>,
}

#[derive(Serialize)]
struct CaptureStatus {
    paused: bool,
    blocklist: crate::config::BlocklistConfig,
    dedup_window_seconds: u32,
}

#[derive(Serialize)]
struct SettingsResponse {
    paused: bool,
    blocklist: crate::config::BlocklistConfig,
    dedup_window_seconds: u32,
    refresh_interval_seconds: u32,
    prune: crate::config::PruneConfig,
    suggestions: crate::config::SuggestionsConfig,
}

async fn pause_capture(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<CaptureStatus>, ApiError> {
    let request = if body.is_empty() {
        PauseRequest { reason: None }
    } else {
        parse_json::<PauseRequest>(&body)?
    };
    if request
        .reason
        .as_ref()
        .is_some_and(|reason| reason.len() > 200)
    {
        return Err(ApiError::invalid_payload(
            "reason max length is 200",
            Some("reason"),
        ));
    }
    {
        let mut capture = state
            .capture
            .lock()
            .map_err(|_| ApiError::internal("capture lock poisoned"))?;
        capture.paused = true;
        persist_capture_config(&state, &capture)?;
    }
    append_audit(&state, "pause".to_string())?;
    capture_status(State(state)).await
}

async fn resume_capture(State(state): State<AppState>) -> Result<Json<CaptureStatus>, ApiError> {
    {
        let mut capture = state
            .capture
            .lock()
            .map_err(|_| ApiError::internal("capture lock poisoned"))?;
        capture.paused = false;
        persist_capture_config(&state, &capture)?;
    }
    append_audit(&state, "resume".to_string())?;
    capture_status(State(state)).await
}

async fn capture_status(State(state): State<AppState>) -> Result<Json<CaptureStatus>, ApiError> {
    let capture = state
        .capture
        .lock()
        .map_err(|_| ApiError::internal("capture lock poisoned"))?;
    Ok(Json(CaptureStatus {
        paused: capture.paused,
        blocklist: capture.blocklist.clone(),
        dedup_window_seconds: capture.dedup_window_seconds,
    }))
}

#[derive(serde::Deserialize)]
struct BlocklistPatch {
    #[serde(default)]
    domains: BlocklistDelta,
    #[serde(default)]
    paths: BlocklistDelta,
    #[serde(default)]
    source_apps: BlocklistDelta,
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

#[derive(Default, serde::Deserialize)]
struct BlocklistDelta {
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

async fn update_blocklist(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<CaptureStatus>, ApiError> {
    let patch: BlocklistPatch = parse_json(&body)?;
    let domain_adds = patch.add.len() + patch.domains.add.len();
    let domain_removes = patch.remove.len() + patch.domains.remove.len();
    let path_adds = patch.paths.add.len();
    let path_removes = patch.paths.remove.len();
    let source_app_adds = patch.source_apps.add.len();
    let source_app_removes = patch.source_apps.remove.len();
    let status = {
        let mut capture = state
            .capture
            .lock()
            .map_err(|_| ApiError::internal("capture lock poisoned"))?;
        apply_blocklist_delta(&mut capture.blocklist.domains, patch.add, patch.remove)?;
        apply_blocklist_delta(
            &mut capture.blocklist.domains,
            patch.domains.add,
            patch.domains.remove,
        )?;
        apply_blocklist_delta(
            &mut capture.blocklist.paths,
            patch.paths.add,
            patch.paths.remove,
        )?;
        apply_blocklist_delta(
            &mut capture.blocklist.source_apps,
            patch.source_apps.add,
            patch.source_apps.remove,
        )?;
        CaptureStatus {
            paused: capture.paused,
            blocklist: capture.blocklist.clone(),
            dedup_window_seconds: capture.dedup_window_seconds,
        }
    };
    {
        let capture = state
            .capture
            .lock()
            .map_err(|_| ApiError::internal("capture lock poisoned"))?;
        persist_capture_config(&state, &capture)?;
    }
    append_audit(
        &state,
        format!(
            "blocklist_update domain_adds={} domain_removes={} path_adds={} path_removes={} source_app_adds={} source_app_removes={}",
            domain_adds, domain_removes, path_adds, path_removes, source_app_adds, source_app_removes
        ),
    )?;
    Ok(Json(status))
}

#[derive(Default, serde::Deserialize)]
struct SettingsPatch {
    paused: Option<bool>,
    blocklist: Option<crate::config::BlocklistConfig>,
    dedup_window_seconds: Option<u32>,
    refresh_interval_seconds: Option<u32>,
    prune: Option<crate::config::PruneConfig>,
    suggestions: Option<SuggestionsPatch>,
}

#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SuggestionsPatch {
    enabled: Option<bool>,
    cli_enabled: Option<bool>,
    ide_enabled: Option<bool>,
    min_chars: Option<u32>,
    limit: Option<u32>,
}

async fn settings(State(state): State<AppState>) -> Result<Json<SettingsResponse>, ApiError> {
    let capture = state
        .capture
        .lock()
        .map_err(|_| ApiError::internal("capture lock poisoned"))?;
    Ok(Json(SettingsResponse {
        paused: capture.paused,
        blocklist: capture.blocklist.clone(),
        dedup_window_seconds: capture.dedup_window_seconds,
        refresh_interval_seconds: capture.ui_refresh_interval_seconds,
        prune: capture.prune.clone(),
        suggestions: capture.suggestions.clone(),
    }))
}

async fn update_settings(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<SettingsResponse>, ApiError> {
    let patch: SettingsPatch = parse_json(&body)?;
    {
        let mut capture = state
            .capture
            .lock()
            .map_err(|_| ApiError::internal("capture lock poisoned"))?;
        if let Some(paused) = patch.paused {
            capture.paused = paused;
        }
        if let Some(blocklist) = patch.blocklist {
            validate_blocklist_config(&blocklist)?;
            capture.blocklist = blocklist;
        }
        if let Some(seconds) = patch.dedup_window_seconds {
            if seconds > 3600 {
                return Err(ApiError::invalid_payload(
                    "dedup_window_seconds must be <= 3600",
                    Some("dedup_window_seconds"),
                ));
            }
            capture.dedup_window_seconds = seconds;
            state
                .db
                .lock()
                .map_err(|_| ApiError::internal("db lock poisoned"))?
                .set_dedup_window_seconds(seconds);
        }
        if let Some(seconds) = patch.refresh_interval_seconds {
            if !(1..=3600).contains(&seconds) {
                return Err(ApiError::invalid_payload(
                    "refresh_interval_seconds must be 1-3600",
                    Some("refresh_interval_seconds"),
                ));
            }
            capture.ui_refresh_interval_seconds = seconds;
        }
        if let Some(prune) = patch.prune {
            validate_prune_settings(&prune)?;
            capture.prune = prune;
        }
        if let Some(suggestions) = patch.suggestions {
            merge_suggestions_settings(&mut capture.suggestions, suggestions)?;
        }
        persist_capture_config(&state, &capture)?;
    }
    append_audit(&state, "settings_update".to_string())?;
    settings(State(state)).await
}

fn persist_capture_config(
    state: &AppState,
    capture: &crate::app::CaptureState,
) -> Result<(), ApiError> {
    let mut config = state.config.as_ref().clone();
    config.capture.paused = capture.paused;
    config.capture.blocklist = capture.blocklist.clone();
    config.capture.dedup_window_seconds = capture.dedup_window_seconds;
    config.ui.refresh_interval_seconds = capture.ui_refresh_interval_seconds;
    config.prune = capture.prune.clone();
    config.suggestions = capture.suggestions.clone();
    config
        .save(&state.paths)
        .map_err(|error| ApiError::internal(error.to_string()))
}

fn validate_blocklist_config(config: &crate::config::BlocklistConfig) -> Result<(), ApiError> {
    for value in config
        .domains
        .iter()
        .chain(config.paths.iter())
        .chain(config.source_apps.iter())
    {
        validate_blocklist_value(value)?;
    }
    Ok(())
}

fn validate_prune_settings(config: &crate::config::PruneConfig) -> Result<(), ApiError> {
    if matches!(config.older_than_days, Some(0)) {
        return Err(ApiError::invalid_payload(
            "older_than_days must be >= 1",
            Some("older_than_days"),
        ));
    }
    if matches!(config.shorter_than, Some(value) if value <= 0) {
        return Err(ApiError::invalid_payload(
            "shorter_than must be positive",
            Some("shorter_than"),
        ));
    }
    if let Some(source) = config.source.as_deref() {
        match source {
            "browser" | "desktop" | "ide" | "cli" | "mcp" => {}
            _ => {
                return Err(ApiError::invalid_payload(
                    "source must be browser, desktop, ide, cli, or mcp",
                    Some("source"),
                ))
            }
        }
    }
    Ok(())
}

fn validate_suggestions_settings(
    config: &crate::config::SuggestionsConfig,
) -> Result<(), ApiError> {
    if !(10..=500).contains(&config.min_chars) {
        return Err(ApiError::invalid_payload(
            "min_chars must be 10-500",
            Some("suggestions.min_chars"),
        ));
    }
    if !(1..=10).contains(&config.limit) {
        return Err(ApiError::invalid_payload(
            "limit must be 1-10",
            Some("suggestions.limit"),
        ));
    }
    Ok(())
}

fn merge_suggestions_settings(
    current: &mut crate::config::SuggestionsConfig,
    patch: SuggestionsPatch,
) -> Result<(), ApiError> {
    let mut next = current.clone();
    if let Some(value) = patch.enabled {
        next.enabled = value;
    }
    if let Some(value) = patch.cli_enabled {
        next.cli_enabled = value;
    }
    if let Some(value) = patch.ide_enabled {
        next.ide_enabled = value;
    }
    if let Some(value) = patch.min_chars {
        next.min_chars = value;
    }
    if let Some(value) = patch.limit {
        next.limit = value;
    }
    validate_suggestions_settings(&next)?;
    *current = next;
    Ok(())
}

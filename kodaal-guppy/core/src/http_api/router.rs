use crate::{
    app::AppState,
    auth,
    db::{
        AddTag, AttachArtifactRequest, CapturePayload, CopyArtifactRequest, PromptQuery,
        PruneRequest, StorageModePatch, SuggestionQuery, UpdateProject, UpdatePrompt,
    },
    error::ApiError,
    ui,
};
use axum::{
    body::Bytes,
    extract::{Multipart, Path, Query, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE, COOKIE, SET_COOKIE},
        HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode,
    },
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashSet,
    fs::OpenOptions,
    io::Write,
    path::{Path as FsPath, PathBuf as FsPathBuf},
    thread,
    time::{Duration as StdDuration, Instant, SystemTime},
};
use tower_http::cors::{AllowOrigin, CorsLayer};

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/prompts", post(create_prompt).get(list_prompts))
        .route("/prompts/suggestions", get(suggest_prompts))
        .route(
            "/prompts/:id",
            get(get_prompt).patch(update_prompt).delete(delete_prompt),
        )
        .route("/prompts/:id/reuse", post(reuse_prompt))
        .route("/prompts/prune", post(prune_prompts))
        .route("/prompts/:id/artifacts", post(attach_artifact))
        .route("/prompts/:id/artifacts/:aid", delete(delete_artifact))
        .route("/artifacts/:aid/copy", post(copy_artifact))
        .route("/artifacts/:aid/content", get(artifact_content))
        .route("/artifacts/:aid", patch(update_artifact_storage))
        .route("/artifacts/verify", post(verify_artifacts))
        .route("/tags", get(list_tags))
        .route("/prompts/:id/tags", post(add_tag))
        .route("/prompts/:id/tags/:tag_id", delete(remove_tag))
        .route("/projects", get(list_projects))
        .route(
            "/projects/:id",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route("/capture/pause", post(pause_capture))
        .route("/capture/resume", post(resume_capture))
        .route("/capture/status", get(capture_status))
        .route("/capture/blocklist", patch(update_blocklist))
        .route("/settings", get(settings).patch(update_settings))
        .route("/stats", get(stats))
        .route("/export", get(export_data))
        .route("/import", post(import_data))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/assets/logo-light.png", get(serve_logo_light))
        .route("/assets/logo-dark.png", get(serve_logo_dark))
        .route("/ui", get(serve_ui))
        .route("/ui/*path", get(serve_ui))
        .nest("/api", api)
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::predicate(|origin, _| {
                    allowed_cors_origin(origin)
                }))
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([CONTENT_TYPE, HeaderName::from_static("x-kodaal-token")]),
        )
        .with_state(state)
}

fn allowed_cors_origin(origin: &HeaderValue) -> bool {
    let Ok(value) = origin.to_str() else {
        return false;
    };
    value == "http://127.0.0.1:7878"
        || value == "http://localhost:7878"
        || value.starts_with("chrome-extension://")
        || value.starts_with("moz-extension://")
}

async fn require_auth<B>(
    State(state): State<AppState>,
    req: Request<B>,
    next: Next<B>,
) -> Response {
    if authenticated(&state, req.headers()) {
        next.run(req).await
    } else {
        ApiError::unauthorized().into_response()
    }
}

fn authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    if let Some(value) = headers
        .get("X-Kodaal-Token")
        .and_then(|value| value.to_str().ok())
    {
        return auth::is_valid_token_value(value) && value == state.token.as_str();
    }
    let Some(cookie) = headers.get(COOKIE).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    cookie.split(';').any(|part| {
        let part = part.trim();
        part.strip_prefix("kg_ui_session=")
            .is_some_and(|value| value == state.ui_session.as_str())
    })
}

async fn healthz(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let schema_version = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .schema_version()?;
    Ok(Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": schema_version
    })))
}

async fn serve_ui(State(state): State<AppState>) -> impl IntoResponse {
    let cookie = format!(
        "kg_ui_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=3600",
        state.ui_session
    );
    let mut response = Html(ui::INDEX_HTML).into_response();
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).expect("static cookie format is valid"),
    );
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

async fn serve_logo_light() -> Response {
    logo_response(include_bytes!("../../assets/logo_light.png"))
}

async fn serve_logo_dark() -> Response {
    logo_response(include_bytes!("../../assets/logo_dark.png"))
}

fn logo_response(bytes: &'static [u8]) -> Response {
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
    response
}

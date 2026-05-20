use axum::{
    body::Body,
    http::{
        header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE, ORIGIN},
        Method, Request,
    },
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs,
    sync::Mutex,
    time::{Duration, Instant},
};
use tower::ServiceExt;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn test_state() -> (tempfile::TempDir, String, axum::Router) {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let dir = tempfile::tempdir().expect("temp dir");
    std::env::set_var("KODAAL_HOME", dir.path());
    let state = kodaal_core::app::AppState::load().expect("app state");
    let token = fs::read_to_string(dir.path().join("token"))
        .expect("token file")
        .trim()
        .to_string();
    let router = kodaal_core::http_api::router(state);
    (dir, token, router)
}

async fn json_response(response: axum::response::Response) -> Value {
    let bytes = hyper::body::to_bytes(response.into_body())
        .await
        .expect("response body");
    serde_json::from_slice(&bytes).expect("json body")
}

async fn api_json(
    app: axum::Router,
    token: &str,
    method: Method,
    uri: &str,
    body: Value,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("X-Kodaal-Token", token)
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .expect("request"),
    )
    .await
    .expect("response")
}

async fn api_empty(
    app: axum::Router,
    token: &str,
    method: Method,
    uri: &str,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("X-Kodaal-Token", token)
            .body(Body::empty())
            .expect("request"),
    )
    .await
    .expect("response")
}

async fn create_prompt(app: axum::Router, token: &str, text: &str) -> Value {
    let response = api_json(
        app,
        token,
        Method::POST,
        "/api/prompts",
        json!({
            "text": text,
            "source": "browser",
            "source_app": "claude.ai",
            "project_hint": {"type": "domain", "value": "claude.ai"}
        }),
    )
    .await;
    assert_eq!(response.status(), 201);
    json_response(response).await
}

async fn create_prompt_payload(app: axum::Router, token: &str, body: Value) -> Value {
    let response = api_json(app, token, Method::POST, "/api/prompts", body).await;
    assert_eq!(response.status(), 201);
    json_response(response).await
}

#[tokio::test]
async fn test_fr105_api_rejects_missing_auth_and_accepts_token() {
    let (_dir, token, app) = test_state();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/prompts")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 401);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/prompts")
                .header("X-Kodaal-Token", token)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "text": "refactor this React component",
                        "source": "browser",
                        "source_app": "claude.ai",
                        "project_hint": {"type": "domain", "value": "claude.ai"},
                        "metadata": {"url": "https://claude.ai/chat/abc"}
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 201);
    let body = json_response(response).await;
    assert_eq!(body["deduped"], false);
    assert_eq!(body["use_count"], 1);
}

#[tokio::test]
async fn test_fr060_fr105_ui_sets_session_cookie_without_exposing_token() {
    let (_dir, token, app) = test_state();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/ui")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
    let cookie = response
        .headers()
        .get("set-cookie")
        .expect("session cookie")
        .to_str()
        .expect("cookie string")
        .to_string();
    let html = hyper::body::to_bytes(response.into_body())
        .await
        .expect("html body");
    let html = String::from_utf8(html.to_vec()).expect("utf8 html");
    assert!(!html.contains(&token));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/capture/status")
                .header("Cookie", cookie)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_fr025_duplicate_prompt_within_window_increments_use_count() {
    let (_dir, token, app) = test_state();
    let payload = json!({
        "text": "same exact prompt",
        "source": "cli",
        "source_app": "claude-code"
    })
    .to_string();

    for expected in [(false, 1), (true, 2)] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/prompts")
                    .header("X-Kodaal-Token", &token)
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.clone()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), 201);
        let body = json_response(response).await;
        assert_eq!(body["deduped"], expected.0);
        assert_eq!(body["use_count"], expected.1);
    }
}

#[tokio::test]
async fn test_fr025_same_text_from_different_origins_keeps_separate_rows() {
    let (_dir, token, app) = test_state();
    for (source, source_app) in [
        ("cli", "claude-code"),
        ("desktop", "codex-desktop"),
        ("ide", "cursor"),
    ] {
        let response = api_json(
            app.clone(),
            &token,
            Method::POST,
            "/api/prompts",
            json!({
                "text": "same text different origin",
                "source": source,
                "source_app": source_app
            }),
        )
        .await;
        assert_eq!(response.status(), 201);
        assert_eq!(json_response(response).await["deduped"], false);
    }
    let prompts = api_empty(app, &token, Method::GET, "/api/prompts").await;
    let prompts = json_response(prompts).await;
    assert_eq!(prompts["total"], 3);
}

#[tokio::test]
async fn test_fr109_sensitive_content_is_redacted_before_storage() {
    let (_dir, token, app) = test_state();
    let secret = format!("sk-{}", "a".repeat(40));
    let response = api_json(
        app.clone(),
        &token,
        Method::POST,
        "/api/prompts",
        json!({
            "text": format!("use {secret} for this test"),
            "source": "cli",
            "source_app": "codex-cli"
        }),
    )
    .await;
    assert_eq!(response.status(), 201);
    let created = json_response(response).await;
    let prompt_id = created["id"].as_str().expect("prompt id");
    let fetched = api_empty(
        app,
        &token,
        Method::GET,
        &format!("/api/prompts/{prompt_id}"),
    )
    .await;
    let fetched = json_response(fetched).await;
    assert_eq!(fetched["redacted"], true);
    assert_eq!(fetched["redaction_reason"], "api-key");
    assert!(fetched["text"]
        .as_str()
        .unwrap()
        .contains("[REDACTED:api-key]"));
    assert!(!fetched["text"].as_str().unwrap().contains(&secret));
}

#[tokio::test]
async fn test_nfr009_audit_log_uses_hash_not_prompt_text() {
    let (dir, token, app) = test_state();
    let secret_text = "secret prompt text must not land in audit log";
    let secret_title = "secret conversation title must not land in audit log";
    let secret_tag = "secret-tag-value";
    let secret_project = "secret project name";
    let secret_path = dir.path().join("secret-project-path");
    fs::create_dir_all(&secret_path).expect("project dir");
    let secret_artifact_path = dir.path().join("secret-artifact.txt");
    fs::write(&secret_artifact_path, "secret artifact body").expect("artifact fixture");

    let created = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": secret_text,
            "source": "cli",
            "source_app": "codex",
            "project_hint": {"type": "path", "value": secret_path.to_string_lossy()}
        }),
    )
    .await;
    let prompt_id = created["id"].as_str().expect("prompt id").to_string();
    let project_id = created["project_id"]
        .as_str()
        .expect("project id")
        .to_string();

    assert_eq!(
        api_json(
            app.clone(),
            &token,
            Method::PATCH,
            &format!("/api/prompts/{prompt_id}"),
            json!({"favorite": true, "conversation_title": secret_title}),
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        api_empty(
            app.clone(),
            &token,
            Method::POST,
            &format!("/api/prompts/{prompt_id}/reuse"),
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        api_json(
            app.clone(),
            &token,
            Method::POST,
            &format!("/api/prompts/{prompt_id}/tags"),
            json!({"name": secret_tag}),
        )
        .await
        .status(),
        200
    );
    let tags = json_response(api_empty(app.clone(), &token, Method::GET, "/api/tags").await).await;
    let tag_id = tags[0]["id"].as_str().expect("tag id").to_string();
    assert_eq!(
        api_empty(
            app.clone(),
            &token,
            Method::DELETE,
            &format!("/api/prompts/{prompt_id}/tags/{tag_id}"),
        )
        .await
        .status(),
        204
    );
    let attached = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{prompt_id}/artifacts"),
        json!({"path": secret_artifact_path.to_string_lossy(), "storage_mode": "reference"}),
    )
    .await;
    assert_eq!(attached.status(), 201);
    let artifact = json_response(attached).await;
    let artifact_id = artifact["id"].as_str().expect("artifact id").to_string();
    assert_eq!(
        api_json(
            app.clone(),
            &token,
            Method::PATCH,
            &format!("/api/artifacts/{artifact_id}"),
            json!({"storage_mode": "snapshot"}),
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        api_json(
            app.clone(),
            &token,
            Method::POST,
            &format!("/api/artifacts/{artifact_id}/copy"),
            json!({"target_project_id": project_id, "on_conflict": "rename"}),
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        api_empty(
            app.clone(),
            &token,
            Method::DELETE,
            &format!("/api/prompts/{prompt_id}/artifacts/{artifact_id}"),
        )
        .await
        .status(),
        204
    );
    assert_eq!(
        api_json(
            app.clone(),
            &token,
            Method::PATCH,
            &format!("/api/projects/{project_id}"),
            json!({"name": secret_project, "color": "#008b8b"}),
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        api_json(
            app.clone(),
            &token,
            Method::PATCH,
            "/api/capture/blocklist",
            json!({"paths": {"add": [secret_path.to_string_lossy()]}}),
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        api_empty(
            app.clone(),
            &token,
            Method::DELETE,
            &format!("/api/projects/{project_id}"),
        )
        .await
        .status(),
        204
    );
    assert_eq!(
        api_empty(
            app,
            &token,
            Method::DELETE,
            &format!("/api/prompts/{prompt_id}"),
        )
        .await
        .status(),
        204
    );

    let audit = fs::read_to_string(dir.path().join("audit.log")).expect("audit log");
    assert!(audit.contains("hash="));
    for event in [
        "capture",
        "prompt_update",
        "prompt_reuse",
        "tag_add",
        "tag_remove",
        "artifact_attach",
        "artifact_storage_update",
        "artifact_copy",
        "artifact_delete",
        "project_update",
        "blocklist_update",
        "project_delete",
        "prompt_delete",
    ] {
        assert!(audit.contains(event), "audit log missing {event}");
    }
    assert!(!audit.contains(secret_text));
    assert!(!audit.contains(secret_title));
    assert!(!audit.contains(secret_tag));
    assert!(!audit.contains(secret_project));
    assert!(!audit.contains(&secret_path.to_string_lossy().to_string()));
    assert!(!audit.contains(&secret_artifact_path.to_string_lossy().to_string()));
}

#[tokio::test]
async fn test_fr054_invalid_payload_and_empty_prune_criteria_are_rejected() {
    let (_dir, token, app) = test_state();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/prompts")
                .header("X-Kodaal-Token", &token)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"text": "", "source": "browser", "source_app": "claude.ai"}).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 400);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/prompts/prune")
                .header("X-Kodaal-Token", token)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"dry_run": false}).to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_fr100_fr101_pause_resume_capture_state() {
    let (_dir, token, app) = test_state();

    let pause = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/capture/pause")
                .header("X-Kodaal-Token", &token)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"reason": "sensitive work"}).to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(pause.status(), 200);
    assert_eq!(json_response(pause).await["paused"], true);

    let blocked = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/prompts")
                .header("X-Kodaal-Token", &token)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"text": "should queue", "source": "browser", "source_app": "claude.ai"})
                        .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(blocked.status(), 409);

    let resume = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/capture/resume")
                .header("X-Kodaal-Token", token)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resume.status(), 200);
    assert_eq!(json_response(resume).await["paused"], false);
}

#[tokio::test]
async fn test_fr065_settings_persist_capture_blocklist_dedup_and_prune_defaults() {
    let (dir, token, app) = test_state();
    let response = api_json(
        app.clone(),
        &token,
        Method::PATCH,
        "/api/settings",
        json!({
            "paused": true,
            "blocklist": {
                "domains": ["claude.ai"],
                "paths": ["C:/private"],
                "source_apps": ["blocked-app"]
            },
            "dedup_window_seconds": 12,
            "prune": {
                "older_than_days": 30,
                "shorter_than": 20,
                "source": "cli",
                "dry_run": true
            }
        }),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body = json_response(response).await;
    assert_eq!(body["paused"], true);
    assert_eq!(body["dedup_window_seconds"], 12);
    assert_eq!(body["prune"]["source"], "cli");

    let config = fs::read_to_string(dir.path().join("config.toml")).expect("config");
    assert!(config.contains("dedup_window_seconds = 12"));
    assert!(config.contains("older_than_days = 30"));
    assert!(config.contains("\"blocked-app\""));

    let status = api_empty(app, &token, Method::GET, "/api/capture/status").await;
    let status = json_response(status).await;
    assert_eq!(status["paused"], true);
    assert_eq!(status["blocklist"]["domains"][0], "claude.ai");
}

#[tokio::test]
async fn test_fr111_cli_and_ide_suggestions_are_setting_gated_and_local_only() {
    let (dir, token, app) = test_state();
    for (source, source_app) in [
        ("cli", "codex-cli"),
        ("ide", "codex-vscode"),
        ("browser", "claude.ai"),
    ] {
        create_prompt_payload(
            app.clone(),
            &token,
            json!({
                "text": "refactor rust sqlx transaction handling in axum service",
                "source": source,
                "source_app": source_app
            }),
        )
        .await;
    }

    let disabled = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts/suggestions?q=refactor%20rust%20sqlx%20transaction&surface=cli",
    )
    .await;
    assert_eq!(disabled.status(), 200);
    let disabled = json_response(disabled).await;
    assert_eq!(disabled["enabled"], false);
    assert_eq!(disabled["items"].as_array().unwrap().len(), 0);

    let settings = api_json(
        app.clone(),
        &token,
        Method::PATCH,
        "/api/settings",
        json!({
            "suggestions": {
                "enabled": true,
                "cli_enabled": true,
                "ide_enabled": true,
                "min_chars": 10,
                "limit": 3
            }
        }),
    )
    .await;
    assert_eq!(settings.status(), 200);
    assert_eq!(
        json_response(settings).await["suggestions"]["enabled"],
        true
    );

    let cli = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts/suggestions?q=refactor%20rust%20sqlx%20transaction&surface=cli",
    )
    .await;
    assert_eq!(cli.status(), 200);
    let cli = json_response(cli).await;
    assert_eq!(cli["enabled"], true);
    assert_eq!(cli["similar_count"], 1);
    assert_eq!(cli["items"][0]["source"], "cli");

    let ide = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts/suggestions?q=refactor%20rust%20sqlx%20transaction&surface=ide",
    )
    .await;
    assert_eq!(ide.status(), 200);
    let ide = json_response(ide).await;
    assert_eq!(ide["items"][0]["source"], "ide");

    let rejected = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts/suggestions?q=refactor%20rust%20sqlx%20transaction&surface=browser",
    )
    .await;
    assert_eq!(rejected.status(), 400);

    let narrowed = api_json(
        app.clone(),
        &token,
        Method::PATCH,
        "/api/settings",
        json!({
            "suggestions": {
                "ide_enabled": false,
                "min_chars": 44,
                "limit": 2
            }
        }),
    )
    .await;
    let narrowed = json_response(narrowed).await;
    assert_eq!(narrowed["suggestions"]["enabled"], true);
    assert_eq!(narrowed["suggestions"]["cli_enabled"], true);
    assert_eq!(narrowed["suggestions"]["ide_enabled"], false);
    assert_eq!(narrowed["suggestions"]["min_chars"], 44);
    assert_eq!(narrowed["suggestions"]["limit"], 2);

    let partial = api_json(
        app,
        &token,
        Method::PATCH,
        "/api/settings",
        json!({"suggestions": {"enabled": false}}),
    )
    .await;
    let partial = json_response(partial).await;
    assert_eq!(partial["suggestions"]["enabled"], false);
    assert_eq!(partial["suggestions"]["cli_enabled"], true);
    assert_eq!(partial["suggestions"]["ide_enabled"], false);
    assert_eq!(partial["suggestions"]["min_chars"], 44);
    assert_eq!(partial["suggestions"]["limit"], 2);

    let audit = fs::read_to_string(dir.path().join("audit.log")).expect("audit log");
    assert!(!audit.contains("refactor%20rust%20sqlx%20transaction"));
    assert!(!audit.contains("refactor rust sqlx transaction"));
}

#[tokio::test]
async fn test_fr009_capture_blocklist_is_structured_and_rejects_domains() {
    let (_dir, token, app) = test_state();

    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/capture/blocklist")
                .header("X-Kodaal-Token", &token)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"domains": {"add": ["claude.ai"]}, "source_apps": {"add": ["blocked-app"]}})
                        .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(status.status(), 200);
    let body = json_response(status).await;
    assert_eq!(body["blocklist"]["domains"][0], "claude.ai");
    assert_eq!(body["blocklist"]["source_apps"][0], "blocked-app");

    let blocked_domain = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/prompts")
                .header("X-Kodaal-Token", &token)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "text": "do not capture",
                        "source": "browser",
                        "source_app": "claude.ai",
                        "project_hint": {"type": "domain", "value": "claude.ai"}
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(blocked_domain.status(), 403);

    let blocked_source_app = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/prompts")
                .header("X-Kodaal-Token", token)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "text": "do not capture either",
                        "source": "mcp",
                        "source_app": "blocked-app"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(blocked_source_app.status(), 403);
}

#[tokio::test]
async fn test_nfr006_cors_allows_local_ui_and_rejects_arbitrary_origins() {
    let (_dir, _token, app) = test_state();

    let allowed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/prompts")
                .header(ORIGIN, "http://127.0.0.1:7878")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        allowed
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("http://127.0.0.1:7878")
    );

    let rejected = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/prompts")
                .header(ORIGIN, "https://evil.example")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert!(rejected
        .headers()
        .get(ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_none());
}

#[tokio::test]
async fn test_fr033_fr034_fr035_tags_and_project_mutation_paths() {
    let (_dir, token, app) = test_state();
    let created = create_prompt(app.clone(), &token, "taggable project prompt").await;
    let id = created["id"].as_str().expect("prompt id").to_string();
    let project_id = created["project_id"]
        .as_str()
        .expect("project id")
        .to_string();

    let tagged = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{id}/tags"),
        json!({"name": "refactor"}),
    )
    .await;
    assert_eq!(tagged.status(), 200);
    let tagged = json_response(tagged).await;
    assert_eq!(tagged["tags"][0], "refactor");

    let tags = api_empty(app.clone(), &token, Method::GET, "/api/tags").await;
    assert_eq!(tags.status(), 200);
    let tags = json_response(tags).await;
    let tag_id = tags[0]["id"].as_str().expect("tag id").to_string();
    assert_eq!(tags[0]["count"], 1);

    let untagged = api_empty(
        app.clone(),
        &token,
        Method::DELETE,
        &format!("/api/prompts/{id}/tags/{tag_id}"),
    )
    .await;
    assert_eq!(untagged.status(), 204);

    let renamed = api_json(
        app.clone(),
        &token,
        Method::PATCH,
        &format!("/api/projects/{project_id}"),
        json!({"name": "Claude workspace", "color": "#008b8b"}),
    )
    .await;
    assert_eq!(renamed.status(), 200);
    let renamed = json_response(renamed).await;
    assert_eq!(renamed["name"], "Claude workspace");
    assert_eq!(renamed["color"], "#008b8b");

    let deleted = api_empty(
        app.clone(),
        &token,
        Method::DELETE,
        &format!("/api/projects/{project_id}"),
    )
    .await;
    assert_eq!(deleted.status(), 204);

    let prompt = api_empty(app, &token, Method::GET, &format!("/api/prompts/{id}")).await;
    assert_eq!(prompt.status(), 200);
    let prompt = json_response(prompt).await;
    assert!(prompt["project_id"].is_null());
}

#[tokio::test]
async fn test_fr051_fr052_fr053_prune_dry_run_then_delete() {
    let (_dir, token, app) = test_state();
    let tiny = create_prompt(app.clone(), &token, "tiny").await;
    create_prompt(app.clone(), &token, "long enough prompt text").await;

    let dry_run = api_json(
        app.clone(),
        &token,
        Method::POST,
        "/api/prompts/prune",
        json!({"shorter_than": 10, "dry_run": true}),
    )
    .await;
    assert_eq!(dry_run.status(), 200);
    let dry_run = json_response(dry_run).await;
    assert_eq!(dry_run["deleted"], 1);
    assert_eq!(dry_run["dry_run"], true);

    let before = api_empty(app.clone(), &token, Method::GET, "/api/prompts").await;
    assert_eq!(json_response(before).await["total"], 2);

    let project_dry_run = api_json(
        app.clone(),
        &token,
        Method::POST,
        "/api/prompts/prune",
        json!({"project_id": tiny["project_id"], "dry_run": true}),
    )
    .await;
    assert_eq!(project_dry_run.status(), 200);
    assert_eq!(json_response(project_dry_run).await["deleted"], 2);

    let date_dry_run = api_json(
        app.clone(),
        &token,
        Method::POST,
        "/api/prompts/prune",
        json!({"older_than": "2099-01-01T00:00:00Z", "dry_run": true}),
    )
    .await;
    assert_eq!(date_dry_run.status(), 200);
    assert_eq!(json_response(date_dry_run).await["deleted"], 2);

    let deleted = api_json(
        app.clone(),
        &token,
        Method::POST,
        "/api/prompts/prune",
        json!({"shorter_than": 10, "dry_run": false}),
    )
    .await;
    assert_eq!(deleted.status(), 200);
    assert_eq!(json_response(deleted).await["deleted"], 1);

    let after = api_empty(app, &token, Method::GET, "/api/prompts").await;
    assert_eq!(json_response(after).await["total"], 1);
}

#[tokio::test]
async fn test_fr030_fr031_fr032_fr036_fr040_fr041_fr042_fr043_fr044_fr045_fr046_fr047_fr048_search_organization_filters_sort_and_pagination(
) {
    let (dir, token, app) = test_state();
    let alpha_dir = dir.path().join("alpha");
    let beta_dir = dir.path().join("beta");
    fs::create_dir_all(&alpha_dir).expect("alpha dir");
    fs::create_dir_all(&beta_dir).expect("beta dir");
    let alpha = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "alpha rust refactor hooks",
            "source": "browser",
            "source_app": "claude.ai",
            "project_hint": {"type": "path", "value": alpha_dir.to_string_lossy()}
        }),
    )
    .await;
    let alpha_id = alpha["id"].as_str().expect("alpha id").to_string();
    let alpha_project = alpha["project_id"]
        .as_str()
        .expect("alpha project")
        .to_string();
    let beta = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "beta sqlite schema",
            "source": "cli",
            "source_app": "codex",
            "project_hint": {"type": "path", "value": beta_dir.to_string_lossy()}
        }),
    )
    .await;
    let beta_id = beta["id"].as_str().expect("beta id").to_string();

    api_json(
        app.clone(),
        &token,
        Method::PATCH,
        &format!("/api/prompts/{alpha_id}"),
        json!({"favorite": true}),
    )
    .await;
    api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{alpha_id}/tags"),
        json!({"name": "rust"}),
    )
    .await;
    api_empty(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{alpha_id}/reuse"),
    )
    .await;
    api_empty(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{alpha_id}/reuse"),
    )
    .await;

    let fts = api_empty(app.clone(), &token, Method::GET, "/api/prompts?q=sqlite").await;
    assert_eq!(json_response(fts).await["items"][0]["id"], beta_id);

    let combined = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts?q=refactor&source=browser&favorite=true&tag=rust",
    )
    .await;
    let combined = json_response(combined).await;
    assert_eq!(combined["total"], 1);
    assert_eq!(combined["items"][0]["id"], alpha_id);

    let project_filtered = api_empty(
        app.clone(),
        &token,
        Method::GET,
        &format!("/api/prompts?project_id={alpha_project}"),
    )
    .await;
    assert_eq!(json_response(project_filtered).await["total"], 1);

    let future_from = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts?from=2099-01-01T00:00:00Z",
    )
    .await;
    assert_eq!(json_response(future_from).await["total"], 0);

    let most_used = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts?sort=use_count_desc",
    )
    .await;
    assert_eq!(json_response(most_used).await["items"][0]["id"], alpha_id);

    let oldest = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/prompts?sort=created_asc&limit=1&offset=1",
    )
    .await;
    let oldest = json_response(oldest).await;
    assert_eq!(oldest["limit"], 1);
    assert_eq!(oldest["offset"], 1);

    let favorites = api_empty(app, &token, Method::GET, "/api/prompts?favorite=true").await;
    assert_eq!(json_response(favorites).await["items"][0]["id"], alpha_id);
}

#[tokio::test]
async fn test_fr064_stats_return_platform_activity_and_analytics() {
    let (dir, token, app) = test_state();
    let project_dir = dir.path().join("stats-project");
    fs::create_dir_all(&project_dir).expect("project dir");
    let browser = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "browser prompt with enough text for token estimate",
            "source": "browser",
            "source_app": "claude.ai",
            "project_hint": {"type": "path", "value": project_dir.to_string_lossy()}
        }),
    )
    .await;
    let browser_id = browser["id"].as_str().expect("browser id").to_string();
    let cursor = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "cursor ide prompt",
            "source": "ide",
            "source_app": "cursor",
            "project_hint": {"type": "path", "value": project_dir.to_string_lossy()}
        }),
    )
    .await;
    let cursor_id = cursor["id"].as_str().expect("cursor id").to_string();
    assert_eq!(
        api_empty(
            app.clone(),
            &token,
            Method::POST,
            &format!("/api/prompts/{browser_id}/reuse"),
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        api_empty(
            app.clone(),
            &token,
            Method::DELETE,
            &format!("/api/prompts/{cursor_id}"),
        )
        .await
        .status(),
        204
    );

    let stats = api_empty(app, &token, Method::GET, "/api/stats?range=month").await;
    assert_eq!(stats.status(), 200);
    let stats = json_response(stats).await;
    assert_eq!(stats["range"], "month");
    assert_eq!(stats["total_prompts"], 1);
    assert_eq!(stats["total_copied"], 1);
    assert_eq!(stats["total_deleted"], 1);
    assert!(stats["average_prompts_per_day"].as_f64().unwrap() >= 1.0);
    assert!(stats["average_prompts_per_project"].as_f64().unwrap() >= 1.0);
    assert!(
        stats["average_estimated_tokens_per_prompt"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(stats["most_used_platform"]["source_app"], "claude.ai");
    assert_eq!(stats["by_source_app"][0]["source_app"], "claude.ai");
    assert_eq!(stats["overall_time_series"][0]["count"], 1);
    assert_eq!(stats["platform_time_series"][0]["source"], "browser");
    assert_eq!(stats["provider_time_series"][0]["source_app"], "claude.ai");
    assert_eq!(stats["top_projects"][0]["prompt_count"], 1);
}

#[tokio::test]
async fn test_fr055_fr056_fr057_fr058_export_import_multipart_json() {
    let (_dir, token, app) = test_state();
    create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "export and import this prompt",
            "source": "cli",
            "source_app": "codex"
        }),
    )
    .await;
    create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "browser export filter should skip this",
            "source": "browser",
            "source_app": "claude.ai"
        }),
    )
    .await;

    let export = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/export?format=json&source=cli",
    )
    .await;
    assert_eq!(export.status(), 200);
    let export_bytes = hyper::body::to_bytes(export.into_body())
        .await
        .expect("export body");
    let export_json: Value = serde_json::from_slice(&export_bytes).expect("export json");
    assert_eq!(export_json["prompts"].as_array().expect("prompts").len(), 1);

    let markdown = api_empty(
        app.clone(),
        &token,
        Method::GET,
        "/api/export?format=markdown&source=cli",
    )
    .await;
    assert_eq!(markdown.status(), 200);
    let markdown = hyper::body::to_bytes(markdown.into_body())
        .await
        .expect("markdown body");
    let markdown = String::from_utf8(markdown.to_vec()).expect("markdown");
    assert!(markdown.contains("# Kodaal Guppy Export"));
    assert!(markdown.contains("export and import this prompt"));
    assert!(!markdown.contains("browser export filter should skip this"));

    let boundary = "kodaal-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"export.json\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    body.extend_from_slice(&export_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let imported = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/import")
                .header("X-Kodaal-Token", token)
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(imported.status(), 200);
    let imported = json_response(imported).await;
    assert_eq!(imported["imported"]["prompts"], 1);
}

#[tokio::test]
async fn test_fr050_delete_single_prompt() {
    let (_dir, token, app) = test_state();
    let created = create_prompt(app.clone(), &token, "delete this prompt").await;
    let id = created["id"].as_str().expect("prompt id").to_string();
    let deleted = api_empty(
        app.clone(),
        &token,
        Method::DELETE,
        &format!("/api/prompts/{id}"),
    )
    .await;
    assert_eq!(deleted.status(), 204);
    let missing = api_empty(app, &token, Method::GET, &format!("/api/prompts/{id}")).await;
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn test_fr092_fr093_fr094_fr097_artifact_attach_content_storage_copy_and_delete() {
    let (dir, token, app) = test_state();
    let project_dir = dir.path().join("target-project");
    fs::create_dir_all(&project_dir).expect("project dir");
    let created = api_json(
        app.clone(),
        &token,
        Method::POST,
        "/api/prompts",
        json!({
            "text": "artifact prompt",
            "source": "cli",
            "source_app": "codex",
            "project_hint": {"type": "path", "value": project_dir.to_string_lossy()}
        }),
    )
    .await;
    assert_eq!(created.status(), 201);
    let created = json_response(created).await;
    let prompt_id = created["id"].as_str().expect("prompt id").to_string();
    let project_id = created["project_id"]
        .as_str()
        .expect("project id")
        .to_string();
    let artifact_path = dir.path().join("artifact.txt");
    fs::write(&artifact_path, "artifact body").expect("artifact file");

    let attached = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{prompt_id}/artifacts"),
        json!({"path": artifact_path.to_string_lossy(), "storage_mode": "reference"}),
    )
    .await;
    assert_eq!(attached.status(), 201);
    let attached = json_response(attached).await;
    let artifact_id = attached["id"].as_str().expect("artifact id").to_string();

    let content = api_empty(
        app.clone(),
        &token,
        Method::GET,
        &format!("/api/artifacts/{artifact_id}/content"),
    )
    .await;
    assert_eq!(content.status(), 200);
    let content = hyper::body::to_bytes(content.into_body())
        .await
        .expect("content body");
    assert_eq!(&content[..], b"artifact body");

    let stored = api_json(
        app.clone(),
        &token,
        Method::PATCH,
        &format!("/api/artifacts/{artifact_id}"),
        json!({"storage_mode": "snapshot"}),
    )
    .await;
    assert_eq!(stored.status(), 200);
    assert_eq!(json_response(stored).await["storage_mode"], "snapshot");

    let copied = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/artifacts/{artifact_id}/copy"),
        json!({"target_project_id": project_id, "on_conflict": "rename"}),
    )
    .await;
    assert_eq!(copied.status(), 200);
    assert_eq!(json_response(copied).await["copied"], true);

    let deleted = api_empty(
        app,
        &token,
        Method::DELETE,
        &format!("/api/prompts/{prompt_id}/artifacts/{artifact_id}"),
    )
    .await;
    assert_eq!(deleted.status(), 204);
}

#[tokio::test]
async fn test_fr095_artifact_copy_conflict_modes_are_explicit() {
    let (dir, token, app) = test_state();
    let project_dir = dir.path().join("conflict-project");
    fs::create_dir_all(&project_dir).expect("project dir");
    let created = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "artifact conflict prompt",
            "source": "cli",
            "source_app": "codex-cli",
            "project_hint": {"type": "path", "value": project_dir.to_string_lossy()}
        }),
    )
    .await;
    let prompt_id = created["id"].as_str().expect("prompt id").to_string();
    let project_id = created["project_id"]
        .as_str()
        .expect("project id")
        .to_string();
    let artifact_path = dir.path().join("artifact.txt");
    fs::write(&artifact_path, "new body").expect("artifact file");
    fs::write(project_dir.join("artifact.txt"), "old body").expect("conflict target");
    let attached = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{prompt_id}/artifacts"),
        json!({"path": artifact_path.to_string_lossy(), "storage_mode": "reference"}),
    )
    .await;
    assert_eq!(attached.status(), 201);
    let artifact_id = json_response(attached).await["id"]
        .as_str()
        .expect("artifact id")
        .to_string();

    let prompt_conflict = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/artifacts/{artifact_id}/copy"),
        json!({"target_project_id": project_id.clone(), "on_conflict": "prompt"}),
    )
    .await;
    assert_eq!(prompt_conflict.status(), 409);
    assert_eq!(
        json_response(prompt_conflict).await["error"]["code"],
        "FILE_EXISTS"
    );

    let skipped = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/artifacts/{artifact_id}/copy"),
        json!({"target_project_id": project_id.clone(), "on_conflict": "skip"}),
    )
    .await;
    assert_eq!(skipped.status(), 200);
    assert_eq!(json_response(skipped).await["copied"], false);
    assert_eq!(
        fs::read_to_string(project_dir.join("artifact.txt")).unwrap(),
        "old body"
    );

    let overwritten = api_json(
        app,
        &token,
        Method::POST,
        &format!("/api/artifacts/{artifact_id}/copy"),
        json!({"target_project_id": project_id, "on_conflict": "overwrite"}),
    )
    .await;
    assert_eq!(overwritten.status(), 200);
    assert_eq!(json_response(overwritten).await["copied"], true);
    assert_eq!(
        fs::read_to_string(project_dir.join("artifact.txt")).unwrap(),
        "new body"
    );
}

#[tokio::test]
async fn test_fr061_fr067_ui_contains_multi_select_controls_and_keyboard_shortcut() {
    let (_dir, _token, app) = test_state();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/ui")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
    let html = hyper::body::to_bytes(response.into_body())
        .await
        .expect("html body");
    let html = String::from_utf8(html.to_vec()).expect("utf8 html");
    assert!(html.contains("id=\"select-all\""));
    assert!(html.contains("id=\"bulk-delete\""));
    assert!(html.contains("id=\"sidebar-toggle\""));
    assert!(html.contains("/assets/logo-light.png"));
    assert!(html.contains("projectMenu(project)"));
    assert!(html.contains("[\"tags\", \"Tags\", \"hash\""));
    assert!(html.contains("renderTagDirectory()"));
    assert!(!html.contains("project-delete"));
    assert!(!html.contains("id=\"filter-icon\""));
    assert!(html.contains("line-axis-label"));
    assert!(html.contains("\"tag-form\""));
    assert!(html.contains("addPromptTag(prompt.id"));
    assert!(html.contains("removePromptTag(prompt.id"));
    assert!(html.contains("toggleCaptureFromSettings"));
    assert!(html.contains("toggleSuggestionSettings"));
    assert!(html.contains("editBlocklistSettings"));
    assert!(html.contains("event.key.toLowerCase() === \"j\""));
    assert!(html.contains("event.key.toLowerCase() === \"k\""));
    assert!(html.contains("event.key.toLowerCase() === \"f\""));
    assert!(html.contains("event.key.toLowerCase() === \"d\""));
    assert!(html.contains("artifact.project_id !== currentProjectId"));
    assert!(html.contains("event.key.toLowerCase() === \"a\""));
}

#[tokio::test]
async fn test_ui_logo_assets_are_served_as_png() {
    let (_dir, _token, app) = test_state();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/assets/logo-light.png")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let bytes = hyper::body::to_bytes(response.into_body())
        .await
        .expect("logo body");
    assert!(bytes.len() > 1000);
}

#[test]
fn test_fr026_nfr010_prompt_survives_service_restart_after_capture() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let dir = tempfile::tempdir().expect("temp dir");
    std::env::set_var("KODAAL_HOME", dir.path());

    let state = kodaal_core::app::AppState::load().expect("first app state");
    let created = state
        .db
        .lock()
        .expect("db lock")
        .ingest_prompt(kodaal_core::db::CapturePayload {
            text: "durable prompt after restart".to_string(),
            source: "cli".to_string(),
            source_app: "codex-cli".to_string(),
            project_hint: None,
            conversation_id: None,
            conversation_title: None,
            metadata: None,
        })
        .expect("captured prompt");
    let prompt_id = created.id;

    let reloaded = kodaal_core::app::AppState::load().expect("second app state");
    let fetched = reloaded
        .db
        .lock()
        .expect("db lock")
        .get_prompt(&prompt_id)
        .expect("prompt after reload");
    assert_eq!(fetched.text, "durable prompt after restart");
}

#[test]
fn test_fr027_db_location_resolves_home_and_os_default() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let saved_home = std::env::var_os("KODAAL_HOME");
    let saved_config = std::env::var_os("KODAAL_CONFIG");
    let saved_token = std::env::var_os("KODAAL_TOKEN_FILE");
    let saved_appdata = std::env::var_os("APPDATA");
    let saved_user_home = std::env::var_os("HOME");
    let dir = tempfile::tempdir().expect("temp dir");

    std::env::set_var("KODAAL_HOME", dir.path());
    let paths = kodaal_core::paths::AppPaths::resolve().expect("home paths");
    assert_eq!(paths.db_path, dir.path().join("guppy.db"));
    assert_eq!(paths.token_path, dir.path().join("token"));

    std::env::remove_var("KODAAL_HOME");
    std::env::remove_var("KODAAL_CONFIG");
    std::env::remove_var("KODAAL_TOKEN_FILE");
    #[cfg(windows)]
    {
        std::env::set_var("APPDATA", dir.path().join("AppData").join("Roaming"));
        let paths = kodaal_core::paths::AppPaths::resolve().expect("windows default paths");
        assert_eq!(
            paths.db_path,
            dir.path()
                .join("AppData")
                .join("Roaming")
                .join("Kodaal")
                .join("guppy.db")
        );
    }
    #[cfg(not(windows))]
    {
        std::env::set_var("HOME", dir.path().join("home"));
        let paths = kodaal_core::paths::AppPaths::resolve().expect("unix default paths");
        assert_eq!(
            paths.db_path,
            dir.path().join("home").join(".kodaal").join("guppy.db")
        );
    }

    restore_env("KODAAL_HOME", saved_home);
    restore_env("KODAAL_CONFIG", saved_config);
    restore_env("KODAAL_TOKEN_FILE", saved_token);
    restore_env("APPDATA", saved_appdata);
    restore_env("HOME", saved_user_home);
}

#[test]
fn test_fr028_fr029_nfr012_startup_migrates_and_backup_restores() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let dir = tempfile::tempdir().expect("temp dir");
    std::env::set_var("KODAAL_HOME", dir.path());
    fs::create_dir_all(dir.path().join("backups")).expect("backup dir");
    let db_path = dir.path().join("guppy.db");
    {
        let conn = rusqlite::Connection::open(&db_path).expect("legacy db");
        conn.execute_batch(include_str!("../migrations/V001__initial_schema.sql"))
            .expect("v1 schema");
        conn.execute(
            "CREATE TABLE refinery_schema_history (version INTEGER PRIMARY KEY, name TEXT NOT NULL, applied_on TEXT NOT NULL)",
            [],
        )
        .expect("history table");
        conn.execute(
            "INSERT INTO refinery_schema_history (version, name, applied_on) VALUES (1, 'V001__initial_schema', '2026-05-05T00:00:00.000Z')",
            [],
        )
        .expect("history row");
    }

    let state = kodaal_core::app::AppState::load().expect("migrated state");
    let schema_version = state
        .db
        .lock()
        .expect("db lock")
        .schema_version()
        .expect("schema version");
    assert_eq!(schema_version, 5);

    let backup = fs::read_dir(dir.path().join("backups"))
        .expect("backup entries")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.starts_with("guppy-pre-V"))
        })
        .expect("pre-migration backup");
    let restored = dir.path().join("restored.db");
    fs::copy(&backup, &restored).expect("restore backup");
    let conn = rusqlite::Connection::open(restored).expect("restored db");
    let restored_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history",
            [],
            |row| row.get(0),
        )
        .expect("restored schema version");
    assert_eq!(restored_version, 1);
}

#[test]
fn test_fr110_sqlcipher_requires_key_and_opens_with_env_key() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let saved_home = std::env::var_os("KODAAL_HOME");
    let saved_key = std::env::var_os("KODAAL_TEST_DB_KEY");
    let dir = tempfile::tempdir().expect("temp dir");
    std::env::set_var("KODAAL_HOME", dir.path());
    std::env::remove_var("KODAAL_TEST_DB_KEY");
    fs::write(
        dir.path().join("config.toml"),
        r#"[database]
encryption = "sqlcipher"
key_env = "KODAAL_TEST_DB_KEY"
"#,
    )
    .expect("config");

    let missing_key = kodaal_core::app::AppState::load();
    assert!(missing_key.is_err());

    std::env::set_var("KODAAL_TEST_DB_KEY", "test-passphrase");
    let state = kodaal_core::app::AppState::load().expect("encrypted state");
    let schema_version = state
        .db
        .lock()
        .expect("db lock")
        .schema_version()
        .expect("schema version");
    assert_eq!(schema_version, 5);

    restore_env("KODAAL_HOME", saved_home);
    restore_env("KODAAL_TEST_DB_KEY", saved_key);
}

#[tokio::test]
async fn test_fr090_fr091_auto_artifact_watcher_links_created_files_and_respects_ignores() {
    let (dir, token, app) = test_state();
    let project_dir = dir.path().join("artifact-project");
    fs::create_dir_all(project_dir.join("node_modules")).expect("node modules dir");
    fs::write(project_dir.join(".gitignore"), "*.tmp\nignored-dir/\n").expect("gitignore");
    fs::create_dir_all(project_dir.join("ignored-dir")).expect("ignored dir");

    let created = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "create an implementation plan artifact",
            "source": "ide",
            "source_app": "cursor",
            "project_hint": {"type": "cwd", "value": project_dir.to_string_lossy()}
        }),
    )
    .await;
    let prompt_id = created["id"].as_str().expect("prompt id").to_string();
    std::thread::sleep(Duration::from_millis(400));
    fs::write(project_dir.join("issues.md"), "linked").expect("linked artifact");
    fs::write(
        project_dir.join("node_modules").join("ignored.js"),
        "ignored",
    )
    .expect("node module file");
    fs::write(project_dir.join("ignored.tmp"), "ignored").expect("gitignored file");
    fs::write(project_dir.join("ignored-dir").join("nested.md"), "ignored")
        .expect("ignored nested file");

    let mut prompt = Value::Null;
    for _ in 0..20 {
        let fetched = api_empty(
            app.clone(),
            &token,
            Method::GET,
            &format!("/api/prompts/{prompt_id}"),
        )
        .await;
        assert_eq!(fetched.status(), 200);
        prompt = json_response(fetched).await;
        if prompt["artifacts"].as_array().expect("artifacts").len() == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    let artifacts = prompt["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["filename"], "issues.md");
    assert_eq!(artifacts[0]["project_id"], created["project_id"]);
}

#[tokio::test]
async fn test_fr098_fr099_broken_link_verifier_and_snapshot_size_cap() {
    let (dir, token, app) = test_state();
    let project_dir = dir.path().join("artifact-verify-project");
    fs::create_dir_all(&project_dir).expect("project dir");
    let created = create_prompt_payload(
        app.clone(),
        &token,
        json!({
            "text": "verify artifact handling",
            "source": "ide",
            "source_app": "vscode",
            "project_hint": {"type": "cwd", "value": project_dir.to_string_lossy()}
        }),
    )
    .await;
    let prompt_id = created["id"].as_str().expect("prompt id").to_string();
    let artifact_path = project_dir.join("linked.txt");
    fs::write(&artifact_path, "linked").expect("artifact file");
    let large_path = project_dir.join("too-large.bin");
    fs::write(&large_path, vec![b'x'; (5 * 1024 * 1024) + 1]).expect("large artifact");

    let too_large = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{prompt_id}/artifacts"),
        json!({"path": large_path.to_string_lossy(), "storage_mode": "snapshot"}),
    )
    .await;
    assert_eq!(too_large.status(), 413);
    assert_eq!(
        json_response(too_large).await["error"]["code"],
        "ARTIFACT_TOO_LARGE"
    );

    let attached = api_json(
        app.clone(),
        &token,
        Method::POST,
        &format!("/api/prompts/{prompt_id}/artifacts"),
        json!({"path": artifact_path.to_string_lossy(), "storage_mode": "reference"}),
    )
    .await;
    assert_eq!(attached.status(), 201);
    fs::remove_file(&artifact_path).expect("remove linked source");
    let verified = api_empty(app.clone(), &token, Method::POST, "/api/artifacts/verify").await;
    assert_eq!(verified.status(), 200);
    let verified = json_response(verified).await;
    assert_eq!(verified["checked"], 1);
    assert_eq!(verified["broken"], 1);

    let fetched = api_empty(
        app,
        &token,
        Method::GET,
        &format!("/api/prompts/{prompt_id}"),
    )
    .await;
    let fetched = json_response(fetched).await;
    assert_eq!(fetched["artifacts"][0]["is_broken"], true);
}

#[tokio::test]
async fn test_nfr008_audit_log_is_append_only_for_mutations() {
    let (dir, token, app) = test_state();
    create_prompt(app.clone(), &token, "append only audit seed").await;
    let before = fs::read_to_string(dir.path().join("audit.log")).expect("audit before");
    let pause = api_empty(app.clone(), &token, Method::POST, "/api/capture/pause").await;
    assert_eq!(pause.status(), 200);
    let resume = api_empty(app, &token, Method::POST, "/api/capture/resume").await;
    assert_eq!(resume.status(), 200);
    let after = fs::read_to_string(dir.path().join("audit.log")).expect("audit after");
    assert!(after.starts_with(&before));
    assert!(after.len() > before.len());
}

#[tokio::test]
async fn test_nfr013_concurrent_writes_keep_all_unique_prompts() {
    let (_dir, token, app) = test_state();
    let mut tasks = tokio::task::JoinSet::new();
    for surface in 0..4 {
        let app = app.clone();
        let token = token.clone();
        tasks.spawn(async move {
            for index in 0..100 {
                let response = api_json(
                    app.clone(),
                    &token,
                    Method::POST,
                    "/api/prompts",
                    json!({
                        "text": format!("concurrent prompt {surface}-{index}"),
                        "source": "cli",
                        "source_app": format!("concurrent-{surface}")
                    }),
                )
                .await;
                assert_eq!(response.status(), 201);
            }
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.expect("concurrent task");
    }
    let prompts = api_empty(app, &token, Method::GET, "/api/prompts?limit=1").await;
    assert_eq!(prompts.status(), 200);
    let prompts = json_response(prompts).await;
    assert_eq!(prompts["total"], 400);
}

#[tokio::test]
async fn test_nfr001_nfr002_search_and_capture_latency_smoke() {
    if std::env::var_os("KODAAL_COVERAGE_RUN").is_some() {
        return;
    }
    if std::env::var_os("KODAAL_PERF_SMOKE").is_none() {
        return;
    }
    let (_dir, token, app) = test_state();
    let mut capture_times = Vec::new();
    for index in 0..250 {
        let started = Instant::now();
        let response = api_json(
            app.clone(),
            &token,
            Method::POST,
            "/api/prompts",
            json!({
                "text": format!("latency smoke prompt {index} sqlite refactor project"),
                "source": "cli",
                "source_app": "perf-smoke"
            }),
        )
        .await;
        assert_eq!(response.status(), 201);
        capture_times.push(started.elapsed());
    }

    let mut search_times = Vec::new();
    for _ in 0..50 {
        let started = Instant::now();
        let response = api_empty(app.clone(), &token, Method::GET, "/api/prompts?q=sqlite").await;
        assert_eq!(response.status(), 200);
        let body = json_response(response).await;
        assert!(body["total"].as_i64().expect("total") >= 250);
        search_times.push(started.elapsed());
    }

    let capture_p95 = p95(capture_times);
    let search_p95 = p95(search_times);
    let capture_limit = if cfg!(debug_assertions) {
        Duration::from_millis(100)
    } else {
        Duration::from_millis(20)
    };
    assert!(
        capture_p95 < capture_limit,
        "capture p95 {:?} exceeded {:?}",
        capture_p95,
        capture_limit
    );
    assert!(
        search_p95 < Duration::from_millis(200),
        "search p95 {:?} exceeded 200ms",
        search_p95
    );
}

fn restore_env(key: &str, value: Option<OsString>) {
    if let Some(value) = value {
        std::env::set_var(key, value);
    } else {
        std::env::remove_var(key);
    }
}

fn p95(mut values: Vec<Duration>) -> Duration {
    values.sort();
    values[((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)]
}

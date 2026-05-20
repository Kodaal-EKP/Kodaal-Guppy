use crate::tray::{PromptPreview, TrayMode, TraySnapshot};
use kodaal_core::{
    auth,
    local_api::{query_encode, LocalApiClient, LocalApiError},
    paths::AppPaths,
};
use serde_json::{json, Value};
use std::fs;

pub trait TrayApi {
    fn snapshot(&self) -> Result<TraySnapshot, TrayClientError>;
    fn pause_capture(&self) -> Result<TraySnapshot, TrayClientError>;
    fn resume_capture(&self) -> Result<TraySnapshot, TrayClientError>;
    fn reuse_prompt(&self, id: &str) -> Result<(), TrayClientError>;
    fn token(&self) -> Result<String, TrayClientError>;
}

impl<T: TrayApi + ?Sized> TrayApi for Box<T> {
    fn snapshot(&self) -> Result<TraySnapshot, TrayClientError> {
        (**self).snapshot()
    }

    fn pause_capture(&self) -> Result<TraySnapshot, TrayClientError> {
        (**self).pause_capture()
    }

    fn resume_capture(&self) -> Result<TraySnapshot, TrayClientError> {
        (**self).resume_capture()
    }

    fn reuse_prompt(&self, id: &str) -> Result<(), TrayClientError> {
        (**self).reuse_prompt(id)
    }

    fn token(&self) -> Result<String, TrayClientError> {
        (**self).token()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayClientError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for TrayClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for TrayClientError {}

#[derive(Debug, Clone)]
pub struct GuppyApiClient {
    inner: LocalApiClient,
}

impl GuppyApiClient {
    pub fn from_default_paths() -> Result<Self, TrayClientError> {
        Ok(Self {
            inner: LocalApiClient::from_default_paths().map_err(TrayClientError::from)?,
        })
    }
}

impl TrayApi for GuppyApiClient {
    fn snapshot(&self) -> Result<TraySnapshot, TrayClientError> {
        let health = self.inner.get_json("/healthz")?;
        let status = self.inner.get_json("/api/capture/status")?;
        let stats = self.inner.get_json("/api/stats?range=day")?;
        let prompts = self
            .inner
            .get_json("/api/prompts?limit=5&sort=created_desc")?;
        parse_snapshot(&health, &status, &stats, &prompts)
    }

    fn pause_capture(&self) -> Result<TraySnapshot, TrayClientError> {
        self.inner
            .post_json("/api/capture/pause", json!({ "reason": "tray" }))?;
        self.snapshot()
    }

    fn resume_capture(&self) -> Result<TraySnapshot, TrayClientError> {
        self.inner.post_json("/api/capture/resume", json!({}))?;
        self.snapshot()
    }

    fn reuse_prompt(&self, id: &str) -> Result<(), TrayClientError> {
        self.inner.post_json(
            &format!("/api/prompts/{}/reuse", query_encode(id)),
            json!({}),
        )?;
        Ok(())
    }

    fn token(&self) -> Result<String, TrayClientError> {
        let paths = AppPaths::resolve().map_err(|error| TrayClientError {
            code: "TOKEN_PATH_UNAVAILABLE".to_string(),
            message: error.to_string(),
        })?;
        let token = fs::read_to_string(paths.token_path)
            .map_err(|error| TrayClientError {
                code: "TOKEN_UNAVAILABLE".to_string(),
                message: error.to_string(),
            })?
            .trim()
            .to_string();
        if !auth::is_valid_token_value(&token) {
            return Err(TrayClientError {
                code: "TOKEN_INVALID".to_string(),
                message: "token file contains an invalid token".to_string(),
            });
        }
        Ok(token)
    }
}

fn parse_snapshot(
    health: &Value,
    status: &Value,
    stats: &Value,
    prompts: &Value,
) -> Result<TraySnapshot, TrayClientError> {
    let version = health
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string();
    let paused = status
        .get("paused")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid_core_response("capture status missing paused"))?;
    let prompts_today = stats
        .get("total_prompts")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as u64;
    let recent_prompts = parse_recent_prompts(prompts);

    Ok(TraySnapshot {
        version,
        prompts_today,
        mode: if paused {
            TrayMode::Paused
        } else {
            TrayMode::Capturing
        },
        recent_prompts,
    })
}

fn parse_recent_prompts(value: &Value) -> Vec<PromptPreview> {
    let Some(items) = value.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .take(5)
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?.to_string();
            let text = item.get("text")?.as_str()?.to_string();
            Some(PromptPreview { id, text })
        })
        .collect()
}

fn invalid_core_response(message: impl Into<String>) -> TrayClientError {
    TrayClientError {
        code: "INVALID_CORE_RESPONSE".to_string(),
        message: message.into(),
    }
}

impl From<LocalApiError> for TrayClientError {
    fn from(error: LocalApiError) -> Self {
        Self {
            code: error.code.to_string(),
            message: error.message,
        }
    }
}

impl From<TrayClientError> for TraySnapshot {
    fn from(error: TrayClientError) -> Self {
        TraySnapshot::error(env!("CARGO_PKG_VERSION"), error.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_fr102_api_snapshot_parses_capture_status_and_recent_prompts() {
        let snapshot = parse_snapshot(
            &json!({ "version": "0.1.0" }),
            &json!({ "paused": false }),
            &json!({ "total_prompts": 7 }),
            &json!({
                "items": [
                    { "id": "a", "text": "first" },
                    { "id": "b", "text": "second" }
                ]
            }),
        )
        .expect("snapshot");

        assert_eq!(snapshot.mode, TrayMode::Capturing);
        assert_eq!(snapshot.prompts_today, 7);
        assert_eq!(snapshot.recent_prompts.len(), 2);
        assert_eq!(snapshot.recent_prompts[0].id, "a");
    }

    #[test]
    fn test_fr102_api_snapshot_rejects_missing_capture_paused_flag() {
        let error = parse_snapshot(
            &json!({ "version": "0.1.0" }),
            &json!({}),
            &json!({ "total_prompts": 0 }),
            &json!({ "items": [] }),
        )
        .expect_err("missing paused flag fails");

        assert_eq!(error.code, "INVALID_CORE_RESPONSE");
    }
}

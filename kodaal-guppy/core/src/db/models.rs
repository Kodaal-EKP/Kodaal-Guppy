use crate::{config::Config, error::ApiError, ids, paths::AppPaths};
use chrono::{Duration, Utc};
use rusqlite::{
    params, params_from_iter,
    types::{Value as SqlValue, ValueRef},
    Connection, OptionalExtension,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const V001_SQL: &str = include_str!("../../migrations/V001__initial_schema.sql");
const V002_SQL: &str = include_str!("../../migrations/V002__add_artifacts.sql");
const V003_SQL: &str = include_str!("../../migrations/V003__watcher_offsets.sql");
const V004_SQL: &str = include_str!("../../migrations/V004__add_desktop_source.sql");
const V005_SQL: &str = include_str!("../../migrations/V005__prompt_redaction.sql");

#[derive(Debug)]
pub struct Database {
    conn: Connection,
    dedup_window_seconds: u32,
    max_prompt_length: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Prompt {
    pub id: String,
    pub text: String,
    pub source: String,
    pub source_app: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub conversation_id: Option<String>,
    pub conversation_title: Option<String>,
    pub use_count: i64,
    pub favorite: bool,
    pub tags: Vec<String>,
    pub artifacts: Vec<ArtifactSummary>,
    pub metadata: Map<String, Value>,
    pub redacted: bool,
    pub redaction_reason: Option<String>,
    pub created_at: String,
    pub last_used_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactSummary {
    pub id: String,
    pub filename: String,
    pub project_id: Option<String>,
    pub storage_mode: String,
    pub is_broken: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Artifact {
    pub id: String,
    pub prompt_id: String,
    pub filename: String,
    pub original_path: String,
    pub project_id: Option<String>,
    pub storage_mode: String,
    pub snapshot_size: Option<i64>,
    pub mime_type: Option<String>,
    pub detection_mode: String,
    pub is_broken: bool,
    pub created_at: String,
    pub last_verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptList {
    pub items: Vec<Prompt>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SuggestionQuery {
    pub q: String,
    pub surface: String,
    pub source_app: Option<String>,
    pub project_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptSuggestionList {
    pub enabled: bool,
    pub surface: String,
    pub similar_count: i64,
    pub min_chars: u32,
    pub items: Vec<PromptSuggestion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptSuggestion {
    pub id: String,
    pub text: String,
    pub source: String,
    pub source_app: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub use_count: i64,
    pub favorite: bool,
    pub tags: Vec<String>,
    pub score: f64,
    pub matched_terms: Vec<String>,
    pub created_at: String,
    pub last_used_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub color: String,
    pub prompt_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapturePayload {
    pub text: String,
    pub source: String,
    pub source_app: String,
    pub project_hint: Option<ProjectHint>,
    pub conversation_id: Option<String>,
    pub conversation_title: Option<String>,
    #[serde(default)]
    pub metadata: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectHint {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureResponse {
    pub id: String,
    pub deduped: bool,
    pub use_count: i64,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PromptQuery {
    pub q: Option<String>,
    pub project_id: Option<String>,
    pub source: Option<String>,
    pub source_app: Option<String>,
    pub tag: Option<String>,
    pub favorite: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePrompt {
    pub favorite: Option<bool>,
    pub conversation_title: Option<Option<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddTag {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PruneRequest {
    pub older_than: Option<String>,
    pub shorter_than: Option<i64>,
    pub project_id: Option<String>,
    pub source: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PruneResponse {
    pub deleted: i64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttachArtifactRequest {
    pub path: String,
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageModePatch {
    pub storage_mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CopyArtifactRequest {
    pub target_project_id: String,
    #[serde(default = "default_on_conflict")]
    pub on_conflict: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopyArtifactResponse {
    pub copied: bool,
    pub target_path: String,
    pub renamed_from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactVerification {
    pub checked: i64,
    pub broken: i64,
    pub repaired: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportSummary {
    pub imported: ImportCounts,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportCounts {
    pub prompts: i64,
    pub projects: i64,
    pub tags: i64,
    pub artifacts: i64,
}

async fn attach_artifact(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<crate::db::Artifact>), ApiError> {
    let request: AttachArtifactRequest = parse_json(&body)?;
    let artifact = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .attach_artifact(&id, request)?;
    append_audit(
        &state,
        format!(
            "artifact_attach artifact_id={} prompt_id={}",
            artifact.id, id
        ),
    )?;
    Ok((StatusCode::CREATED, Json(artifact)))
}

async fn delete_artifact(
    State(state): State<AppState>,
    Path((id, aid)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .delete_artifact(&id, &aid)?;
    append_audit(
        &state,
        format!("artifact_delete artifact_id={} prompt_id={}", aid, id),
    )?;
    Ok(StatusCode::NO_CONTENT)
}

async fn copy_artifact(
    State(state): State<AppState>,
    Path(aid): Path<String>,
    body: Bytes,
) -> Result<Json<crate::db::CopyArtifactResponse>, ApiError> {
    let request: CopyArtifactRequest = parse_json(&body)?;
    let response = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .copy_artifact(&aid, request)?;
    append_audit(
        &state,
        format!(
            "artifact_copy artifact_id={} copied={} target_path_hash={}",
            aid,
            response.copied,
            crate::ids::sha256_hex(&response.target_path)
        ),
    )?;
    Ok(Json(response))
}

async fn artifact_content(
    State(state): State<AppState>,
    Path(aid): Path<String>,
) -> Result<Response, ApiError> {
    let (bytes, content_type) = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .artifact_content(&aid)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .map_err(|error| ApiError::internal(error.to_string()))?,
    );
    Ok((headers, bytes).into_response())
}

async fn update_artifact_storage(
    State(state): State<AppState>,
    Path(aid): Path<String>,
    body: Bytes,
) -> Result<Json<crate::db::Artifact>, ApiError> {
    let request: StorageModePatch = parse_json(&body)?;
    let storage_mode = request.storage_mode.clone();
    let artifact = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .update_artifact_storage(&aid, request)?;
    append_audit(
        &state,
        format!(
            "artifact_storage_update artifact_id={} storage_mode={}",
            aid, storage_mode
        ),
    )?;
    Ok(Json(artifact))
}

async fn verify_artifacts(
    State(state): State<AppState>,
) -> Result<Json<crate::db::ArtifactVerification>, ApiError> {
    let response = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .verify_artifact_links()?;
    append_audit(
        &state,
        format!(
            "artifact_verify checked={} broken={} repaired={}",
            response.checked, response.broken, response.repaired
        ),
    )?;
    Ok(Json(response))
}

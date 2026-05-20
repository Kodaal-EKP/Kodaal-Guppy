async fn list_tags(State(state): State<AppState>) -> Result<Json<Vec<crate::db::Tag>>, ApiError> {
    Ok(Json(
        state
            .db
            .lock()
            .map_err(|_| ApiError::internal("db lock poisoned"))?
            .list_tags()?,
    ))
}

async fn add_tag(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<crate::db::Prompt>, ApiError> {
    let request: AddTag = parse_json(&body)?;
    let tag_hash = crate::ids::sha256_hex(&request.name);
    let prompt = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .add_tag(&id, &request.name)?;
    append_audit(
        &state,
        format!("tag_add prompt_id={} tag_hash={}", id, tag_hash),
    )?;
    Ok(Json(prompt))
}

async fn remove_tag(
    State(state): State<AppState>,
    Path((id, tag_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .remove_tag(&id, &tag_id)?;
    append_audit(
        &state,
        format!("tag_remove prompt_id={} tag_id={}", id, tag_id),
    )?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::db::Project>>, ApiError> {
    Ok(Json(
        state
            .db
            .lock()
            .map_err(|_| ApiError::internal("db lock poisoned"))?
            .list_projects()?,
    ))
}

async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::db::Project>, ApiError> {
    Ok(Json(
        state
            .db
            .lock()
            .map_err(|_| ApiError::internal("db lock poisoned"))?
            .get_project(&id)?,
    ))
}

async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Json<crate::db::Project>, ApiError> {
    let request: UpdateProject = parse_json(&body)?;
    let name_changed = request.name.is_some();
    let color_changed = request.color.is_some();
    let project = state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .update_project(&id, request)?;
    append_audit(
        &state,
        format!(
            "project_update project_id={} name_changed={} color_changed={}",
            id, name_changed, color_changed
        ),
    )?;
    Ok(Json(project))
}

async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .lock()
        .map_err(|_| ApiError::internal("db lock poisoned"))?
        .delete_project(&id)?;
    append_audit(&state, format!("project_delete project_id={}", id))?;
    Ok(StatusCode::NO_CONTENT)
}

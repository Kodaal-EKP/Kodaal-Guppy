fn search_prompts(api: &LocalApiClient, args: &Value) -> Result<Value, McpFailure> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| McpFailure::invalid("query is required"))?;
    let limit = bounded_limit(args.get("limit").and_then(Value::as_i64), 10)?;
    let mut params = vec![
        format!("q={}", query_encode(query)),
        format!("limit={limit}"),
        "sort=created_desc".to_string(),
    ];
    if let Some(source) = args.get("source").and_then(Value::as_str) {
        params.push(format!("source={}", query_encode(source)));
    }
    if args
        .get("favorite_only")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        params.push("favorite=true".to_string());
    }
    if let Some(project) = args.get("project").and_then(Value::as_str) {
        if let Some(project_id) = resolve_project(api, project)? {
            params.push(format!("project_id={}", query_encode(&project_id)));
        }
    }
    let response = api
        .get_json(&format!("/api/prompts?{}", params.join("&")))
        .map_err(McpFailure::from)?;
    Ok(json!({
        "matches": prompt_matches(&response),
        "total_matches": response.get("total").and_then(Value::as_i64).unwrap_or(0)
    }))
}

fn recent_prompts(api: &LocalApiClient, args: &Value) -> Result<Value, McpFailure> {
    let limit = bounded_limit(args.get("limit").and_then(Value::as_i64), 10)?;
    let mut params = vec![format!("limit={limit}"), "sort=created_desc".to_string()];
    if let Some(project) = args.get("project").and_then(Value::as_str) {
        if let Some(project_id) = resolve_project(api, project)? {
            params.push(format!("project_id={}", query_encode(&project_id)));
        }
    }
    if let Some(since) = args.get("since").and_then(Value::as_str) {
        params.push(format!("from={}", query_encode(since)));
    }
    let response = api
        .get_json(&format!("/api/prompts?{}", params.join("&")))
        .map_err(McpFailure::from)?;
    Ok(json!({
        "matches": prompt_matches(&response),
        "total_matches": response.get("total").and_then(Value::as_i64).unwrap_or(0)
    }))
}

fn reuse_prompt(api: &LocalApiClient, args: &Value) -> Result<Value, McpFailure> {
    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| McpFailure::invalid("id is required"))?;
    let prompt = api
        .get_json(&format!("/api/prompts/{id}"))
        .map_err(McpFailure::from)?;
    let reuse = api
        .post_json(&format!("/api/prompts/{id}/reuse"), json!({}))
        .map_err(McpFailure::from)?;
    Ok(json!({
        "id": prompt.get("id").cloned().unwrap_or(Value::Null),
        "text": prompt.get("text").cloned().unwrap_or(Value::String(String::new())),
        "new_use_count": reuse.get("use_count").cloned().unwrap_or(Value::Null),
        "metadata": {
            "source": prompt.get("source").cloned().unwrap_or(Value::Null),
            "source_app": prompt.get("source_app").cloned().unwrap_or(Value::Null),
            "project_name": prompt.get("project_name").cloned().unwrap_or(Value::Null),
            "created_at": prompt.get("created_at").cloned().unwrap_or(Value::Null),
            "tags": prompt.get("tags").cloned().unwrap_or(json!([]))
        }
    }))
}

fn prompt_matches(response: &Value) -> Vec<Value> {
    response
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|prompt| {
                    json!({
                        "id": prompt.get("id").cloned().unwrap_or(Value::Null),
                        "text": prompt.get("text").cloned().unwrap_or(Value::String(String::new())),
                        "project_name": prompt.get("project_name").cloned().unwrap_or(Value::Null),
                        "source": prompt.get("source").cloned().unwrap_or(Value::Null),
                        "source_app": prompt.get("source_app").cloned().unwrap_or(Value::Null),
                        "created_at": prompt.get("created_at").cloned().unwrap_or(Value::Null),
                        "use_count": prompt.get("use_count").cloned().unwrap_or(Value::Null),
                        "favorite": prompt.get("favorite").cloned().unwrap_or(Value::Bool(false))
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_project(api: &LocalApiClient, value: &str) -> Result<Option<String>, McpFailure> {
    let projects = api.get_json("/api/projects").map_err(McpFailure::from)?;
    Ok(projects.as_array().and_then(|items| {
        items.iter().find_map(|project| {
            let id = project.get("id").and_then(Value::as_str)?;
            let name = project.get("name").and_then(Value::as_str).unwrap_or("");
            if id == value || name == value {
                Some(id.to_string())
            } else {
                None
            }
        })
    }))
}

fn bounded_limit(value: Option<i64>, default: i64) -> Result<i64, McpFailure> {
    let value = value.unwrap_or(default);
    if (1..=50).contains(&value) {
        Ok(value)
    } else {
        Err(McpFailure::invalid("limit must be 1-50"))
    }
}

fn first_line(text: &str) -> String {
    let mut line = text.lines().next().unwrap_or("Prompt").trim().to_string();
    if line.len() > 80 {
        line.truncate(77);
        line.push_str("...");
    }
    line
}

fn jsonrpc_response(id: Value, result: Result<Value, McpFailure>) -> Value {
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32603,
                "message": "Tool execution failed",
                "data": {
                    "kodaal_code": error.code,
                    "kodaal_message": error.message
                }
            }
        }),
    }
}

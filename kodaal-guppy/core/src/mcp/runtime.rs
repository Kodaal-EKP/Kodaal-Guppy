use crate::local_api::{query_encode, LocalApiClient, LocalApiError};
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Write};

pub fn run_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut server = McpServer::new();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = server.handle_line(&line) {
            writeln!(stdout, "{}", response)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct McpServer {
    client_name: String,
}

impl McpServer {
    fn new() -> Self {
        Self {
            client_name: "mcp-client".to_string(),
        }
    }

    fn handle_line(&mut self, line: &str) -> Option<String> {
        let request = serde_json::from_str::<Value>(line).ok()?;
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        id.as_ref()?;
        let response = match method {
            "initialize" => self.initialize(&request),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.call_tool(&request),
            "resources/list" => self.list_resources(),
            "resources/read" => self.read_resource(&request),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err(McpFailure {
                code: "METHOD_NOT_FOUND",
                message: format!("unsupported MCP method {method}"),
            }),
        };
        Some(jsonrpc_response(id.unwrap_or(Value::Null), response).to_string())
    }

    fn initialize(&mut self, request: &Value) -> Result<Value, McpFailure> {
        if let Some(name) = request
            .pointer("/params/clientInfo/name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
        {
            self.client_name = name.chars().take(120).collect();
        }
        Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "resources": {},
                "prompts": {}
            },
            "serverInfo": {
                "name": "kodaal-guppy",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    fn call_tool(&self, request: &Value) -> Result<Value, McpFailure> {
        let name = request
            .pointer("/params/name")
            .and_then(Value::as_str)
            .ok_or_else(|| McpFailure::invalid("tool name is required"))?;
        let args = request
            .pointer("/params/arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let api = LocalApiClient::from_default_paths().map_err(McpFailure::from)?;
        let output = match name {
            "log_prompt" => self.log_prompt(&api, &args)?,
            "search_prompts" => search_prompts(&api, &args)?,
            "get_recent_prompts" => recent_prompts(&api, &args)?,
            "reuse_prompt" => reuse_prompt(&api, &args)?,
            _ => {
                return Err(McpFailure {
                    code: "TOOL_NOT_FOUND",
                    message: format!("unknown tool {name}"),
                })
            }
        };
        Ok(json!({
            "content": [
                { "type": "text", "text": serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string()) }
            ]
        }))
    }

    fn log_prompt(&self, api: &LocalApiClient, args: &Value) -> Result<Value, McpFailure> {
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| McpFailure::invalid("text is required"))?;
        let mut body = Map::new();
        body.insert("text".to_string(), Value::String(text.to_string()));
        body.insert("source".to_string(), Value::String("mcp".to_string()));
        body.insert(
            "source_app".to_string(),
            Value::String(self.client_name.clone()),
        );
        if let Some(project_hint) = args.get("project_hint") {
            body.insert("project_hint".to_string(), project_hint.clone());
        }
        if let Some(value) = args.get("conversation_id").and_then(Value::as_str) {
            body.insert(
                "conversation_id".to_string(),
                Value::String(value.to_string()),
            );
        }
        if let Some(value) = args.get("conversation_title").and_then(Value::as_str) {
            body.insert(
                "conversation_title".to_string(),
                Value::String(value.to_string()),
            );
        }
        api.post_json("/api/prompts", Value::Object(body))
            .map_err(McpFailure::from)
    }

    fn list_resources(&self) -> Result<Value, McpFailure> {
        let api = LocalApiClient::from_default_paths().map_err(McpFailure::from)?;
        let prompts = api
            .get_json("/api/prompts?limit=50&sort=created_desc")
            .map_err(McpFailure::from)?;
        let projects = api.get_json("/api/projects").map_err(McpFailure::from)?;
        let mut resources = Vec::new();
        for prompt in prompts
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            if let Some(id) = prompt.get("id").and_then(Value::as_str) {
                resources.push(json!({
                    "uri": format!("kodaal://prompts/{id}"),
                    "name": first_line(prompt.get("text").and_then(Value::as_str).unwrap_or("Prompt")),
                    "mimeType": "text/plain"
                }));
            }
        }
        for project in projects.as_array().cloned().unwrap_or_default() {
            if let Some(id) = project.get("id").and_then(Value::as_str) {
                resources.push(json!({
                    "uri": format!("kodaal://projects/{id}"),
                    "name": project.get("name").and_then(Value::as_str).unwrap_or("Project"),
                    "mimeType": "application/json"
                }));
            }
        }
        Ok(json!({ "resources": resources }))
    }

    fn read_resource(&self, request: &Value) -> Result<Value, McpFailure> {
        let uri = request
            .pointer("/params/uri")
            .and_then(Value::as_str)
            .ok_or_else(|| McpFailure::invalid("resource uri is required"))?;
        let api = LocalApiClient::from_default_paths().map_err(McpFailure::from)?;
        if let Some(id) = uri.strip_prefix("kodaal://prompts/") {
            let prompt = api
                .get_json(&format!("/api/prompts/{id}"))
                .map_err(McpFailure::from)?;
            return Ok(json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "text/plain",
                    "text": prompt.get("text").and_then(Value::as_str).unwrap_or("")
                }]
            }));
        }
        if let Some(id) = uri.strip_prefix("kodaal://projects/") {
            let project = api
                .get_json(&format!("/api/projects/{id}"))
                .map_err(McpFailure::from)?;
            return Ok(json!({
                "contents": [{
                    "uri": uri,
                    "mimeType": "application/json",
                    "text": project.to_string()
                }]
            }));
        }
        Err(McpFailure {
            code: "RESOURCE_NOT_FOUND",
            message: format!("unknown resource {uri}"),
        })
    }
}

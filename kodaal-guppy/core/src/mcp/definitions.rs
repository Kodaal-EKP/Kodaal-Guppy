#[derive(Debug, Clone)]
struct McpFailure {
    code: &'static str,
    message: String,
}

impl McpFailure {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "INVALID_PAYLOAD",
            message: message.into(),
        }
    }
}

impl From<LocalApiError> for McpFailure {
    fn from(error: LocalApiError) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "log_prompt",
            "description": "Save a prompt to Kodaal Guppy.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "project_hint": {
                        "type": "object",
                        "properties": {
                            "type": { "enum": ["path", "domain", "cwd"] },
                            "value": { "type": "string" }
                        },
                        "required": ["type", "value"]
                    },
                    "conversation_id": { "type": "string" },
                    "conversation_title": { "type": "string" }
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "search_prompts",
            "description": "Search Kodaal Guppy for past prompts matching a query.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 10 },
                    "project": { "type": "string" },
                    "source": { "enum": ["browser", "desktop", "ide", "cli", "mcp"] },
                    "favorite_only": { "type": "boolean", "default": false }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "get_recent_prompts",
            "description": "Return recent prompts from Kodaal Guppy in reverse chronological order.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 10 },
                    "project": { "type": "string" },
                    "since": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "reuse_prompt",
            "description": "Retrieve a specific prompt by ID and increment its use count.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" }
                },
                "required": ["id"]
            }
        }),
    ]
}

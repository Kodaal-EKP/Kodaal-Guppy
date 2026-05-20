#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fr021_tools_list_exposes_four_tools() {
        let mut server = McpServer::new();
        let response = server
            .handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .expect("response");
        let value: Value = serde_json::from_str(&response).expect("json");
        let tools = value
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().any(|tool| tool["name"] == "log_prompt"));
        assert!(tools.iter().any(|tool| tool["name"] == "search_prompts"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "get_recent_prompts"));
        assert!(tools.iter().any(|tool| tool["name"] == "reuse_prompt"));
    }

    #[test]
    fn test_initialize_records_client_name() {
        let mut server = McpServer::new();
        let _ = server.handle_line(
            r#"{"jsonrpc":"2.0","id":"init","method":"initialize","params":{"clientInfo":{"name":"Claude Desktop"}}}"#,
        );
        assert_eq!(server.client_name, "Claude Desktop");
    }

    #[test]
    fn test_fr023_prompt_matches_do_not_fabricate_score() {
        let matches = prompt_matches(&json!({
            "items": [
                {
                    "id": "11111111-1111-4111-8111-111111111111",
                    "text": "find rust sqlite examples",
                    "source": "mcp",
                    "source_app": "Claude Desktop",
                    "created_at": "2026-05-06T00:00:00Z",
                    "use_count": 1,
                    "favorite": false
                }
            ]
        }));

        assert_eq!(matches.len(), 1);
        assert!(matches[0].get("score").is_none());
    }
}

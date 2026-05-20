use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Mutex,
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct McpHarness {
    _dir: tempfile::TempDir,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    server: tokio::task::JoinHandle<hyper::Result<()>>,
}

impl McpHarness {
    async fn start() -> Self {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("temp dir");
        std::env::set_var("KODAAL_HOME", dir.path());
        let state = kodaal_core::app::AppState::load().expect("app state");
        let token = fs::read_to_string(dir.path().join("token")).expect("token file");
        assert_eq!(token.trim().len(), 64);
        let app = kodaal_core::http_api::router(state);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("local addr").port();
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let server = axum::Server::from_tcp(listener)
            .expect("server")
            .serve(app.into_make_service());
        let server = tokio::spawn(server);

        let mut child = Command::new(env!("CARGO_BIN_EXE_kodaal"))
            .arg("mcp-server")
            .env("KODAAL_HOME", dir.path())
            .env("KODAAL_PORT", port.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("mcp child");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            _dir: dir,
            child,
            stdin,
            stdout,
            server,
        }
    }

    fn request(&mut self, value: Value) -> Value {
        writeln!(self.stdin, "{value}").expect("write request");
        self.stdin.flush().expect("flush request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("json response")
    }

    fn initialize(&mut self) {
        let response = self.request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "stdio-test-client"}}
        }));
        assert_eq!(response["result"]["serverInfo"]["name"], "kodaal-guppy");
    }

    fn call_tool(&mut self, id: i64, name: &str, arguments: Value) -> Value {
        self.request(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
    }
}

impl Drop for McpHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.server.abort();
    }
}

fn tool_text(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("tool text");
    serde_json::from_str(text).expect("tool json text")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_fr022_fr023_fr024_mcp_stdio_tool_flow() {
    let mut harness = McpHarness::start().await;
    harness.initialize();

    let logged = harness.call_tool(
        2,
        "log_prompt",
        json!({
            "text": "mcp stdio saves this exact prompt",
            "project_hint": {"type": "domain", "value": "mcp.local"}
        }),
    );
    assert!(logged.get("error").is_none(), "{logged}");
    let logged = tool_text(&logged);
    let prompt_id = logged["id"].as_str().expect("prompt id").to_string();
    assert_eq!(logged["deduped"], false);

    let searched = harness.call_tool(
        3,
        "search_prompts",
        json!({"query": "stdio", "limit": 5, "source": "mcp"}),
    );
    assert!(searched.get("error").is_none(), "{searched}");
    let searched = tool_text(&searched);
    assert_eq!(searched["total_matches"], 1);
    assert_eq!(searched["matches"][0]["id"], prompt_id);
    assert!(searched["matches"][0].get("score").is_none());

    let recent = harness.call_tool(4, "get_recent_prompts", json!({"limit": 1}));
    assert!(recent.get("error").is_none(), "{recent}");
    let recent = tool_text(&recent);
    assert_eq!(recent["matches"][0]["id"], prompt_id);

    let reused = harness.call_tool(5, "reuse_prompt", json!({"id": prompt_id}));
    assert!(reused.get("error").is_none(), "{reused}");
    let reused = tool_text(&reused);
    assert_eq!(reused["new_use_count"], 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_fr021_mcp_stdio_rejects_unknown_tool() {
    let mut harness = McpHarness::start().await;
    harness.initialize();
    let response = harness.call_tool(2, "unknown_tool", json!({}));
    assert_eq!(response["error"]["data"]["kodaal_code"], "TOOL_NOT_FOUND");
}

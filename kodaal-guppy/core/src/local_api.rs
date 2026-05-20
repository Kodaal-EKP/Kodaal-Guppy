use crate::{config::Config, paths::AppPaths};
use serde_json::Value;
use std::{
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::Path,
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct LocalApiClient {
    port: u16,
    token: String,
}

#[derive(Debug, Clone)]
pub struct LocalApiError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for LocalApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LocalApiError {}

impl LocalApiClient {
    pub fn from_default_paths() -> Result<Self, LocalApiError> {
        let paths = AppPaths::resolve().map_err(|error| LocalApiError {
            code: "MCP_CORE_NOT_INSTALLED",
            message: error.to_string(),
        })?;
        let token = fs::read_to_string(&paths.token_path)
            .map_err(|_| LocalApiError {
                code: "MCP_CORE_NOT_INSTALLED",
                message: format!("token file missing at {}", paths.token_path.display()),
            })?
            .trim()
            .to_string();
        if !crate::auth::is_valid_token_value(&token) {
            return Err(LocalApiError {
                code: "AUTH_TOKEN_INVALID",
                message: "token file contains an invalid token".to_string(),
            });
        }
        let config = if paths.config_path.exists() {
            Config::load_or_create(&paths).map_err(|error| LocalApiError {
                code: "INVALID_CONFIG",
                message: error.to_string(),
            })?
        } else {
            Config::default()
        };
        let port = env::var("KODAAL_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(config.server.port);
        Ok(Self { port, token })
    }

    pub fn get_json(&self, path: &str) -> Result<Value, LocalApiError> {
        self.request_json("GET", path, None)
    }

    pub fn get_text(&self, path: &str) -> Result<String, LocalApiError> {
        self.request_text("GET", path, None)
    }

    pub fn post_json(&self, path: &str, body: Value) -> Result<Value, LocalApiError> {
        self.request_json("POST", path, Some(body))
    }

    pub fn patch_json(&self, path: &str, body: Value) -> Result<Value, LocalApiError> {
        self.request_json("PATCH", path, Some(body))
    }

    pub fn delete(&self, path: &str) -> Result<(), LocalApiError> {
        self.request_text("DELETE", path, None).map(|_| ())
    }

    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>, LocalApiError> {
        self.request_bytes("GET", path, "application/json", Vec::new())
    }

    pub fn post_multipart_file(
        &self,
        path: &str,
        field_name: &str,
        file_path: &Path,
    ) -> Result<Value, LocalApiError> {
        let file_bytes = fs::read(file_path).map_err(|error| LocalApiError {
            code: "INVALID_PAYLOAD",
            message: error.to_string(),
        })?;
        let filename = file_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("import.json");
        let boundary = format!("kodaal-{}", crate::auth::generate_token());
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                safe_header_value(field_name),
                safe_header_value(filename)
            )
            .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
        body.extend_from_slice(&file_bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let response = self.request_bytes(
            "POST",
            path,
            &format!("multipart/form-data; boundary={boundary}"),
            body,
        )?;
        if response.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&response).map_err(|error| LocalApiError {
            code: "INVALID_CORE_RESPONSE",
            message: error.to_string(),
        })
    }

    fn request_json(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, LocalApiError> {
        let body = self.request_text(method, path, body)?;
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|error| LocalApiError {
            code: "INVALID_CORE_RESPONSE",
            message: error.to_string(),
        })
    }

    fn request_text(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<String, LocalApiError> {
        let body_text = body.map(|value| value.to_string()).unwrap_or_default();
        let body = self.request_bytes(method, path, "application/json", body_text.into_bytes())?;
        Ok(String::from_utf8_lossy(&body).into_owned())
    }

    fn request_bytes(
        &self,
        method: &str,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, LocalApiError> {
        let mut stream =
            TcpStream::connect(("127.0.0.1", self.port)).map_err(|error| LocalApiError {
                code: "MCP_CORE_UNAVAILABLE",
                message: error.to_string(),
            })?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(internal)?;
        stream
            .set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(internal)?;
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nX-Kodaal-Token: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
            self.port,
            self.token,
            content_type,
            body.len()
        );
        stream.write_all(request.as_bytes()).map_err(internal)?;
        stream.write_all(&body).map_err(internal)?;
        let response = read_http_response(&mut stream)?;
        parse_http_body(&response)
    }
}

fn read_http_response(stream: &mut TcpStream) -> Result<Vec<u8>, LocalApiError> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).map_err(internal)?;
        if read == 0 {
            break find_header_end(&response);
        }
        response.extend_from_slice(&buffer[..read]);
        if let Some(index) = find_header_end(&response) {
            break Some(index);
        }
    }
    .ok_or_else(|| LocalApiError {
        code: "MCP_CORE_UNAVAILABLE",
        message: "invalid HTTP response from core".to_string(),
    })?;
    let content_length = content_length(&response[..header_end]).unwrap_or(0);
    while response.len().saturating_sub(header_end) < content_length {
        let read = stream.read(&mut buffer).map_err(internal)?;
        if read == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..read]);
    }
    Ok(response)
}

fn find_header_end(response: &[u8]) -> Option<usize> {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .or_else(|| {
            response
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| index + 2)
        })
}

fn content_length(header: &[u8]) -> Option<usize> {
    let header = String::from_utf8_lossy(header);
    header.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

fn parse_http_body(response: &[u8]) -> Result<Vec<u8>, LocalApiError> {
    let header_end = find_header_end(response).ok_or_else(|| LocalApiError {
        code: "MCP_CORE_UNAVAILABLE",
        message: "invalid HTTP response from core".to_string(),
    })?;
    let head = String::from_utf8_lossy(&response[..header_end]);
    let length = content_length(&response[..header_end]).unwrap_or(0);
    let body_bytes = &response[header_end..response.len().min(header_end + length)];
    let body = String::from_utf8_lossy(body_bytes);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    if !(200..300).contains(&status) {
        let value = serde_json::from_str::<Value>(&body).unwrap_or(Value::Null);
        let code = value
            .pointer("/error/code")
            .and_then(Value::as_str)
            .unwrap_or("HTTP_ERROR");
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or(&body)
            .to_string();
        return Err(LocalApiError {
            code: stable_code(code),
            message,
        });
    }
    Ok(body_bytes.to_vec())
}

fn stable_code(code: &str) -> &'static str {
    match code {
        "INVALID_PAYLOAD" => "INVALID_PAYLOAD",
        "INVALID_QUERY" => "INVALID_QUERY",
        "UNAUTHORIZED" => "UNAUTHORIZED",
        "CAPTURE_PAUSED" => "CAPTURE_PAUSED",
        "FORBIDDEN" => "FORBIDDEN",
        "PROMPT_NOT_FOUND" => "PROMPT_NOT_FOUND",
        "PROJECT_NOT_FOUND" => "PROJECT_NOT_FOUND",
        _ => "HTTP_ERROR",
    }
}

fn internal(error: impl std::fmt::Display) -> LocalApiError {
    LocalApiError {
        code: "MCP_CORE_UNAVAILABLE",
        message: error.to_string(),
    }
}

pub fn query_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            b' ' => output.push('+'),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn safe_header_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, '"' | '\r' | '\n'))
        .collect()
}

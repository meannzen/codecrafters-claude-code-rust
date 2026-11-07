use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Clone)]
pub struct LspClient {
    address: String,
    path: String,
    pub extension_to_language: HashMap<String, String>,
}

impl LspClient {
    pub async fn from_environment() -> crate::Result<Self> {
        if std::env::var_os("CODECRAFTERS_LSP_URL").is_none() {
            return Err("CODECRAFTERS_LSP_URL is not set".into());
        }
        Ok(Self::new().await?)
    }

    pub async fn new() -> crate::Result<Self> {
        let url = std::env::var("CODECRAFTERS_LSP_URL")?;
        let local = url
            .strip_prefix("http://127.0.0.1:")
            .ok_or("Expected local LSP transport")?;
        let (port, path) = local.split_once('/').unwrap_or((local, ""));
        let mut client = Self {
            address: format!("127.0.0.1:{port}"),
            path: if path.is_empty() {
                String::new()
            } else {
                format!("/{path}")
            },
            extension_to_language: HashMap::new(),
        };

        let configuration = client.call("configuration", &serde_json::json!({})).await?;
        client.extension_to_language =
            serde_json::from_value(configuration["extensionToLanguage"].clone())?;
        Ok(client)
    }

    pub async fn call(&self, action: &str, payload: &Value) -> crate::Result<Value> {
        let body = serde_json::to_string(payload)?;

        let mut stream = timeout(Duration::from_secs(10), TcpStream::connect(&self.address))
            .await
            .map_err(|_| "Connection timed out")??;

        let request = format!(
            "POST {}/{} HTTP/1.0\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.path,
            action,
            self.address,
            body.len(),
            body
        );

        // Write request with a 10-second timeout
        timeout(
            Duration::from_secs(10),
            stream.write_all(request.as_bytes()),
        )
        .await
        .map_err(|_| "Write timed out")??;

        let mut response = Vec::new();
        timeout(Duration::from_secs(10), stream.read_to_end(&mut response))
            .await
            .map_err(|_| "Read timed out")??;

        let response_str = String::from_utf8(response)?;
        let (headers, result) = response_str
            .split_once("\r\n\r\n")
            .ok_or("Invalid transport response")?;

        if headers.split_whitespace().nth(1) != Some("200") {
            return Err(result.to_owned().into());
        }

        Ok(serde_json::from_str(result)?)
    }
}

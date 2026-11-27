use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use url::Url;

use crate::diagnostics::DiagnosticTracker;

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
        Self::new().await
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

/// Open documents (URI -> last version sent) shared by every tool that talks to the server.
pub struct LspDocuments {
    client: LspClient,
    versions: Mutex<HashMap<String, u32>>,
    diagnostics: Mutex<DiagnosticTracker>,
}

impl LspDocuments {
    pub fn new(client: LspClient) -> Self {
        Self {
            client,
            versions: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(DiagnosticTracker::default()),
        }
    }

    pub async fn new_diagnostics(&self, wait_ms: u64) -> Option<String> {
        let work_dir = std::env::current_dir().ok()?;
        let versions = self.versions.lock().await;
        let mut tracker = self.diagnostics.lock().await;
        tracker
            .collect(&self.client, &versions, &work_dir, wait_ms)
            .await
    }

    pub fn client(&self) -> &LspClient {
        &self.client
    }

    pub fn supports(&self, path: &Path) -> bool {
        self.language_id(path).is_some()
    }

    fn language_id(&self, path: &Path) -> Option<&str> {
        let ext = path.extension()?.to_str()?;
        self.client
            .extension_to_language
            .get(&format!(".{ext}"))
            .map(String::as_str)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({ "method": method, "params": params });
        self.client
            .call("notify", &payload)
            .await
            .map(|_| ())
            .map_err(|e| format!("{method} failed: {e:?}"))
    }

    async fn did_open(&self, path: &Path, uri: &Url, text: &str) -> Result<(), String> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri.as_str(),
                    "languageId": self.language_id(path).unwrap_or("plaintext"),
                    "version": 1,
                    "text": text,
                }
            }),
        )
        .await
    }

    /// Opens the document from disk if the server has not seen it yet.
    pub async fn ensure_open(&self, path: &Path, uri: &Url) -> Result<(), String> {
        let mut versions = self.versions.lock().await;
        if versions.contains_key(uri.as_str()) {
            return Ok(());
        }

        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        self.did_open(path, uri, &text).await?;
        versions.insert(uri.to_string(), 1);
        Ok(())
    }

    /// Sends the full saved text: didChange with the next version, or didOpen if not open yet.
    pub async fn sync_saved(&self, path: &Path, uri: &Url, text: &str) -> Result<(), String> {
        let mut versions = self.versions.lock().await;

        let version = match versions.get(uri.as_str()).copied() {
            Some(current) => {
                let next = current + 1;
                self.notify(
                    "textDocument/didChange",
                    json!({
                        "textDocument": { "uri": uri.as_str(), "version": next },
                        "contentChanges": [{ "text": text }],
                    }),
                )
                .await?;
                next
            }
            None => {
                self.did_open(path, uri, text).await?;
                1
            }
        };

        versions.insert(uri.to_string(), version);
        Ok(())
    }
}

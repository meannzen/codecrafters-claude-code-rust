use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use url::Url;

use crate::lsp_client::LspClient;

#[derive(Debug, Deserialize)]
struct Position {
    line: u32,
    character: u32,
}

#[derive(Debug, Deserialize)]
struct Range {
    start: Position,
}

#[derive(Debug, Deserialize)]
struct RawDiagnostic {
    range: Range,
    message: String,
}

#[derive(Debug, Deserialize)]
struct PublishParams {
    uri: String,
    version: Option<i64>,
    diagnostics: Vec<RawDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Diagnostic {
    line: u32,
    character: u32,
    message: String,
}

#[derive(Default)]
pub struct DiagnosticTracker {
    cursor: u64,
    shown: HashMap<String, HashSet<Diagnostic>>,
}

impl DiagnosticTracker {
    pub async fn collect(
        &mut self,
        client: &LspClient,
        versions: &HashMap<String, u32>,
        work_dir: &Path,
    ) -> Option<String> {
        let response = client
            .call(
                "notifications",
                &json!({ "after": self.cursor, "timeoutMs": 0 }),
            )
            .await
            .ok()?;

        if let Some(cursor) = response.get("cursor").and_then(Value::as_u64) {
            self.cursor = cursor;
        }
        let notifications = response.get("notifications").and_then(Value::as_array)?;

        let mut report: Vec<(String, Vec<Diagnostic>)> = Vec::new();

        for n in notifications {
            if n.get("method").and_then(Value::as_str) != Some("textDocument/publishDiagnostics") {
                continue;
            }
            let Some(params) = n
                .get("params")
                .and_then(|p| PublishParams::deserialize(p).ok())
            else {
                continue;
            };

            if let (Some(v), Some(sent)) = (params.version, versions.get(&params.uri))
                && v != i64::from(*sent)
            {
                continue;
            }

            let mut seen = HashSet::new();
            let current: Vec<Diagnostic> = params
                .diagnostics
                .into_iter()
                .map(|d| Diagnostic {
                    line: d.range.start.line + 1,
                    character: d.range.start.character + 1,
                    message: d.message,
                })
                .filter(|d| seen.insert(d.clone()))
                .collect();

            let previous = self.shown.get(&params.uri);
            let fresh: Vec<Diagnostic> = current
                .iter()
                .filter(|d| !previous.is_some_and(|p| p.contains(*d)))
                .cloned()
                .collect();
            self.shown.insert(params.uri.clone(), seen);

            let file = display_path(&params.uri, work_dir);
            report.retain(|(f, _)| *f != file);
            if !fresh.is_empty() {
                report.push((file, fresh));
            }
        }

        if report.is_empty() {
            return None;
        }

        let mut out = String::from(
            "<new-diagnostics>The following new diagnostic issues were detected by the language server. Use them to repair your edits, including callers in files you did not edit:\n",
        );
        for (file, diags) in report {
            out.push_str(&format!("\n{file}:"));
            for d in diags {
                out.push_str(&format!(
                    "\n  [Line {}:{}] {}",
                    d.line, d.character, d.message
                ));
            }
        }
        out.push_str("\n</new-diagnostics>");
        Some(out)
    }
}

fn display_path(uri: &str, work_dir: &Path) -> String {
    let path: Option<PathBuf> = Url::parse(uri).ok().and_then(|u| u.to_file_path().ok());
    match &path {
        Some(p) => p.strip_prefix(work_dir).unwrap_or(p).display().to_string(),
        None => uri.to_string(),
    }
}

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use crate::lsp_client::LspDocuments;

pub struct LspTool {
    docs: Arc<LspDocuments>,
}

impl LspTool {
    pub fn new(docs: Arc<LspDocuments>) -> Self {
        Self { docs }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LspArgs {
    operation: Operation,
    file_path: String,
    /// 1-based
    line: u32,
    /// 1-based, UTF-16 code units
    character: u32,
}

#[derive(Debug, Deserialize)]
enum Operation {
    #[serde(rename = "goToDefinition")]
    GoToDefinition,

    #[serde(rename = "findReferences")]
    FindReferences,
}

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
struct ReferenceLocation {
    uri: String,
    range: Range,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DefinitionTarget {
    Link {
        #[serde(rename = "targetUri")]
        target_uri: String,
        #[serde(rename = "targetSelectionRange")]
        target_selection_range: Range,
    },
    Location {
        uri: String,
        range: Range,
    },
}

impl DefinitionTarget {
    fn into_parts(self) -> (String, Position) {
        match self {
            Self::Link {
                target_uri,
                target_selection_range,
            } => (target_uri, target_selection_range.start),
            Self::Location { uri, range } => (uri, range.start),
        }
    }
}

/// Result is `Location | Location[] | LocationLink[] | null`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DefinitionResponse {
    Many(Vec<DefinitionTarget>),
    One(DefinitionTarget),
}

impl DefinitionResponse {
    fn first(self) -> Option<DefinitionTarget> {
        match self {
            Self::Many(v) => v.into_iter().next(),
            Self::One(t) => Some(t),
        }
    }
}

#[async_trait]
impl super::Tool for LspTool {
    fn name(&self) -> &str {
        "LSP"
    }

    fn definition(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "LSP",
                "description": "Code intelligence for a symbol at a position. goToDefinition returns where the symbol is defined; findReferences returns every place it is used. Never guess positions: pass the exact line and character that goToDefinition returned (1-based) to findReferences. Positions shift after edits, so after Write edits run goToDefinition again on a caller (e.g. the call site) to get the updated definition position, then run findReferences at that exact position.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["goToDefinition", "findReferences"]
                        },
                        "filePath": {
                            "type": "string",
                            "description": "Absolute path, or a path relative to the agent's current working directory."
                        },
                        "line": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "1-based line number of the symbol."
                        },
                        "character": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "1-based character offset in UTF-16 code units (same as character count for ASCII)."
                        }
                    },
                    "required": ["operation", "filePath", "line", "character"]
                }
            }
        })
    }

    async fn execute(&self, args: &Value) -> Result<String, String> {
        let args = LspArgs::deserialize(args).map_err(|e| format!("Invalid LSP arguments: {e}"))?;
        match args.operation {
            Operation::GoToDefinition => self.go_to_definition(&args).await,
            Operation::FindReferences => self.references(&args).await,
        }
    }
}

impl LspTool {
    async fn references(&self, args: &LspArgs) -> Result<String, String> {
        let work_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to read current directory: {e}"))?;
        let abs_path = work_dir.join(&args.file_path);
        let uri = Url::from_file_path(&abs_path)
            .map_err(|_| format!("Not a valid file path: {}", abs_path.display()))?;

        self.open_document(&abs_path, &uri).await?;

        let payload = json!({
            "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": args.line.saturating_sub(1),
                    "character": args.character.saturating_sub(1),
                },
                "context": { "includeDeclaration": true }
            }
        });

        let response = self
            .docs
            .client()
            .call("request", &payload)
            .await
            .map_err(|e| format!("textDocument/references failed: {e:?}"))?;

        let locations = serde_json::from_value::<Option<Vec<ReferenceLocation>>>(response)
            .map_err(|e| format!("Unexpected references response: {e}"))?
            .unwrap_or_default();

        Ok(format_references(locations, &work_dir))
    }

    async fn go_to_definition(&self, args: &LspArgs) -> Result<String, String> {
        let work_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to read current directory: {e}"))?;
        let abs_path = work_dir.join(&args.file_path);
        let uri = Url::from_file_path(&abs_path)
            .map_err(|_| format!("Not a valid file path: {}", abs_path.display()))?;

        self.open_document(&abs_path, &uri).await?;

        let payload = json!({
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": uri.as_str() },
                "position": {
                    "line": args.line.saturating_sub(1),
                    "character": args.character.saturating_sub(1),
                }
            }
        });

        let response = self
            .docs
            .client()
            .call("request", &payload)
            .await
            .map_err(|e| format!("textDocument/definition failed: {e:?}"))?;

        let target = serde_json::from_value::<Option<DefinitionResponse>>(response)
            .map_err(|e| format!("Unexpected definition response: {e}"))?
            .and_then(DefinitionResponse::first);

        Ok(match target {
            Some(t) => {
                let (uri, pos) = t.into_parts();
                format_location(&uri, &pos, &work_dir)
            }
            None => "No definition found".to_string(),
        })
    }

    async fn open_document(&self, path: &Path, uri: &Url) -> Result<(), String> {
        self.docs.ensure_open(path, uri).await
    }
}

fn display_path(uri: &str, work_dir: &Path) -> String {
    let path: Option<PathBuf> = Url::parse(uri).ok().and_then(|u| u.to_file_path().ok());

    match &path {
        Some(p) => p.strip_prefix(work_dir).unwrap_or(p).display().to_string(),
        None => uri.to_string(),
    }
}

fn format_references(locations: Vec<ReferenceLocation>, work_dir: &Path) -> String {
    if locations.is_empty() {
        return "No references found".to_string();
    }

    let total = locations.len();
    let mut files: Vec<(String, Vec<Position>)> = Vec::new();
    for loc in locations {
        let file = display_path(&loc.uri, work_dir);
        match files.iter_mut().find(|(f, _)| *f == file) {
            Some((_, positions)) => positions.push(loc.range.start),
            None => files.push((file, vec![loc.range.start])),
        }
    }

    let mut out = format!(
        "Found {total} {} across {} {}:",
        if total == 1 {
            "reference"
        } else {
            "references"
        },
        files.len(),
        if files.len() == 1 { "file" } else { "files" },
    );
    for (file, positions) in &files {
        out.push_str(&format!("\n\n{file}:"));
        for pos in positions {
            out.push_str(&format!("\n  Line {}:{}", pos.line + 1, pos.character + 1));
        }
    }
    out
}

/// Formats a 0-based LSP position as `path:line:char` (1-based),
/// relative to `work_dir` when possible.
fn format_location(uri: &str, pos: &Position, work_dir: &Path) -> String {
    let display = display_path(uri, work_dir);

    format!(
        "Defined in {}:{}:{} (use filePath {}, line {}, character {} for findReferences; positions are 1-based and change after edits)",
        display,
        pos.line + 1,
        pos.character + 1,
        display,
        pos.line + 1,
        pos.character + 1
    )
}

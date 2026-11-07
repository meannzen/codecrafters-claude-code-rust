use async_trait::async_trait;
use serde_json::{Value, json};

pub struct LspTool;

#[async_trait]
impl super::Tool for LspTool {
    fn name(&self) -> &str {
        "LSP"
    }
    fn definition(&self) -> serde_json::Value {
        json!(
            {
              "type": "function",
              "function": {
                "name": "LSP",
                "description": "Returns the file, line, and character where a symbol is defined.",
                "parameters": {
                  "type": "object",
                  "properties": {
                    "operation": {
                      "type": "string",
                      "enum": ["goToDefinition"]
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
            }
        )
    }
    async fn execute(&self, _args: &Value) -> Result<String, String> {
        Ok("LSP client connected".to_string())
    }
}

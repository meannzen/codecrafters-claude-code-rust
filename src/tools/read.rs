use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs;

pub struct ReadTool;

#[async_trait]
impl super::Tool for ReadTool {
    fn name(&self) -> &str {
        "Read"
    }

    fn definition(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "Read",
                "description": "Read and return the contents of a file",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "The path to the file to read"
                        }
                    },
                    "required": ["file_path"]
                }
            }
        })
    }

    async fn execute(&self, args: &Value) -> Result<String, String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Read called without file_path".to_string())?;

        fs::read_to_string(file_path).map_err(|err| format!("Error reading file: {}", err))
    }
}

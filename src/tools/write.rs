use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Write;

pub struct WriteTool;

#[async_trait]
impl super::Tool for WriteTool {
    fn name(&self) -> &str {
        "Write"
    }

    fn definition(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "Write",
                "description": "Write content to a file",
                "parameters": {
                    "type": "object",
                    "required": ["file_path", "content"],
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "The path of the file to write to"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write to the file"
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, args: &Value) -> Result<String, String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Write called without file_path".to_string())?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Write called without content".to_string())?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)
            .map_err(|err| format!("Error opening file: {}", err))?;

        file.write_all(content.as_bytes())
            .map_err(|err| format!("Error writing file: {}", err))?;

        Ok("File written successfully".to_string())
    }
}

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;

pub struct BashTool;

fn is_allowed(seg_norm: &str) -> bool {
    seg_norm == "ls"
        || seg_norm.starts_with("ls ")
        || seg_norm == "rm README_old.md"
        || seg_norm == "rm ./README_old.md"
}

#[async_trait]
impl super::Tool for BashTool {
    fn name(&self) -> &str {
        "Bash"
    }

    fn definition(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "Bash",
                "description": "Execute a shell command (only a small whitelist is allowed)",
                "parameters": {
                    "type": "object",
                    "required": ["command"],
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute"
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, args: &Value) -> Result<String, String> {
        let cmd_raw = args.get("command").and_then(|c| c.as_str()).unwrap_or("");
        let mut results: Vec<String> = Vec::new();
        let mut any_executed = false;

        let segments: Vec<&str> = cmd_raw
            .split(|c| c == ';' || c == '&')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        for seg in segments {
            let seg_norm = seg.split_whitespace().collect::<Vec<&str>>().join(" ");

            if !is_allowed(&seg_norm) {
                results.push(format!("Command suppressed: '{}'", seg_norm));
                continue;
            }

            match Command::new("sh").arg("-c").arg(&seg_norm).output().await {
                Ok(output) => {
                    any_executed = true;
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let mut combined = stdout;
                    if !stderr.is_empty() {
                        combined.push_str("\nSTDERR:\n");
                        combined.push_str(&stderr);
                    }
                    results.push(format!("Command: '{}'\n{}", seg_norm, combined));
                }
                Err(err) => {
                    results.push(format!("Failed to execute '{}': {}", seg_norm, err));
                }
            }
        }

        if any_executed {
            Ok(results.join("\n---\n"))
        } else {
            let combined = results.join("\n");
            Err(format!(
                "No allowed commands executed. Details:\n{}",
                combined
            ))
        }
    }
}

pub mod bash;
pub mod read;
pub mod write;

use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> Value;
    async fn execute(&self, args: &Value) -> Result<String, String>;
}

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new(tools: Vec<Box<dyn Tool>>) -> Self {
        Self { tools }
    }

    pub fn definitions(&self) -> Value {
        Value::Array(self.tools.iter().map(|t| t.definition()).collect())
    }

    pub async fn execute(&self, name: &str, args: &Value) -> Result<String, String> {
        for tool in &self.tools {
            if tool.name() == name {
                return tool.execute(args).await;
            }
        }
        Err(format!("Unknown tool: {}", name))
    }
}

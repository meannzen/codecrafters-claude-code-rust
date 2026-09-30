use async_trait::async_trait;
use serde_json::{Value, json};

use crate::skill::Skill;

pub struct SkillTool {
    skills: Vec<Skill>,
}

impl SkillTool {
    pub fn new(skills: Vec<Skill>) -> Self {
        Self { skills }
    }
}

#[async_trait]
impl super::Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn definition(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": "Skill",
                "description": "Load a skill's instructions into the conversation",
                "parameters": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "description": "The name of the skill to use" },
                        "args": { "type": "string", "description": "Optional arguments for the skill" }
                    }
                }
            }
        })
    }

    async fn execute(&self, args: &Value) -> Result<String, String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Skill called without name".to_string())?;

        let mut skill = self
            .skills
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .ok_or_else(|| format!("Unknown skill: {name}"))?;

        let arguments = args
            .get("args")
            .and_then(|v| v.as_str())
            .map(|a| a.split_whitespace().map(String::from).collect())
            .unwrap_or_default();

        Ok(skill.add_arguments(arguments).with_location())
    }
}

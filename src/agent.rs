use async_openai::Client;
use async_openai::config::OpenAIConfig;
use futures::future::join_all;
use serde_json::{Value, json};
use std::{future::Future, pin::Pin};

use crate::{skill::Skill, tools::ToolRegistry};

type LoopFuture<'f> =
    Pin<Box<dyn Future<Output = Result<String, Box<dyn std::error::Error>>> + 'f>>;

pub struct Agent<'a> {
    pub client: &'a Client<OpenAIConfig>,
    pub skills: Vec<Skill>,
    pub registry: &'a ToolRegistry,
}

#[derive(Default)]
pub struct AgentBuilder<'a> {
    client: Option<&'a Client<OpenAIConfig>>,
    skills: Vec<Skill>,
    registry: Option<&'a ToolRegistry>,
}

impl<'a> AgentBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn client(mut self, client: &'a Client<OpenAIConfig>) -> Self {
        self.client = Some(client);
        self
    }

    pub fn skills(mut self, skills: Vec<Skill>) -> Self {
        self.skills = skills;
        self
    }

    pub fn registry(mut self, registry: &'a ToolRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn build(self) -> Agent<'a> {
        Agent {
            client: self.client.expect("AgentBuilder: client is required"),
            skills: self.skills,
            registry: self.registry.expect("AgentBuilder: registry is required"),
        }
    }
}

impl<'a> Agent<'a> {
    pub fn builder() -> AgentBuilder<'a> {
        AgentBuilder::new()
    }

    pub async fn run(&self, prompt: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut messages: Vec<Value> = Vec::new();

        if !self.skills.is_empty() {
            let mut sys_content = String::from("You have access to the following skills:\n");
            for skill in &self.skills {
                sys_content.push_str(&format!("\n- {}: {}", skill.name, skill.description));
            }
            sys_content.push_str(
                "\n\nIf a skill matches the user's request, call the Skill tool with its name\nand follow the instructions it returns.",
            );
            messages.push(json!({
                "role": "system",
                "content": sys_content
            }));
        }

        messages.push(json!({ "role": "user", "content": prompt }));

        let answer = self.run_loop(messages, false).await?;
        println!("{answer}");
        Ok(())
    }

    fn run_loop(&self, mut messages: Vec<Value>, is_subagent: bool) -> LoopFuture<'_> {
        Box::pin(async move {
            loop {
                let payload = json!({
                    "messages": messages.clone(),
                    "model": "anthropic/claude-haiku-4.5",
                    "tools": self.tool_definitions(is_subagent)
                });

                let response: Value = self.client.chat().create_byot(payload).await?;

                let choices = match response.get("choices").and_then(|v| v.as_array()) {
                    Some(c) if !c.is_empty() => c,
                    _ => {
                        eprintln!("No 'choices' array in model response: {:#}", response);
                        return Ok(String::new());
                    }
                };

                let message_obj = match choices[0].get("message") {
                    Some(m) => m,
                    None => {
                        eprintln!("Choice without message: {:#}", choices[0]);
                        return Ok(String::new());
                    }
                };

                messages.push(message_obj.clone());

                if let Some(tool_calls) = message_obj.get("tool_calls").and_then(|v| v.as_array()) {
                    let dispatch_futures = tool_calls.iter().map(|tc| self.dispatch_tool_call(tc));
                    let results = join_all(dispatch_futures).await;

                    for (id, content) in results {
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": content
                        }));
                    }
                } else if let Some(content) = message_obj.get("content").and_then(|v| v.as_str()) {
                    return Ok(content.to_string());
                } else {
                    eprintln!("Received message without content or tool_calls");
                    return Ok(String::new());
                }
            }
        })
    }

    async fn dispatch_tool_call(&self, tool_call: &Value) -> (String, String) {
        let id = tool_call
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown_id")
            .to_string();

        let function_obj = match tool_call.get("function") {
            Some(f) => f,
            None => {
                return (
                    id,
                    "Error: missing function object in tool_call".to_string(),
                );
            }
        };

        let function_name = match function_obj.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return (id, "Error: missing function name".to_string()),
        };

        let args_str = match function_obj.get("arguments").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return (id, "Error: missing arguments string".to_string()),
        };

        let args: Value = match serde_json::from_str(args_str) {
            Ok(v) => v,
            Err(err) => {
                return (id, format!("Failed to parse arguments as JSON: {}", err));
            }
        };

        if function_name == "Skill"
            && let Some(skill) = self.forked_skill(&args)
        {
            let content = match self.run_subagent(&skill).await {
                Ok(answer) => format!(
                    "Skill {} ran in a separate context and returned: {}",
                    skill.name, answer
                ),
                Err(err) => format!("Tool execution failed: {}", err),
            };
            return (id, content);
        }

        let content = match self.registry.execute(function_name, &args).await {
            Ok(output) => output,
            Err(err) => format!("Tool execution failed: {}", err),
        };

        (id, content)
    }

    fn tool_definitions(&self, is_subagent: bool) -> Value {
        let mut defs = self.registry.definitions();
        if is_subagent && let Some(list) = defs.as_array_mut() {
            list.retain(|d| d["function"]["name"] != "Skill");
        }
        defs
    }

    fn forked_skill(&self, args: &Value) -> Option<Skill> {
        let name = args.get("name").and_then(|v| v.as_str())?;
        let mut skill = self
            .skills
            .iter()
            .find(|s| s.name == name && s.fork)?
            .clone();
        let arguments = args
            .get("args")
            .and_then(|v| v.as_str())
            .map(|a| a.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        skill.add_arguments(arguments);
        Some(skill)
    }

    async fn run_subagent(&self, skill: &Skill) -> Result<String, Box<dyn std::error::Error>> {
        let messages = vec![json!({ "role": "user", "content": skill.body.trim() })];
        self.run_loop(messages, true).await
    }
}

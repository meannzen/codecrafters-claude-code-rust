use async_openai::Client;
use async_openai::config::OpenAIConfig;
use serde_json::{Value, json};

use crate::tools::ToolRegistry;

pub async fn run(
    client: &Client<OpenAIConfig>,
    prompt: &str,
    registry: &ToolRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut messages: Vec<Value> = vec![json!({ "role": "user", "content": prompt })];

    loop {
        let payload = json!({
            "messages": messages.clone(),
            "model": "anthropic/claude-haiku-4.5",
            "tools": registry.definitions()
        });

        let response: Value = client.chat().create_byot(payload).await?;

        let choices = match response.get("choices").and_then(|v| v.as_array()) {
            Some(c) if !c.is_empty() => c.clone(),
            _ => {
                eprintln!("No 'choices' array in model response: {:#}", response);
                return Ok(());
            }
        };

        for choice in choices {
            if let Some(message_obj) = choice.get("message") {
                messages.push(message_obj.clone());

                if let Some(tool_calls) = message_obj.get("tool_calls").and_then(|v| v.as_array())
                {
                    for tool_call in tool_calls {
                        let result = dispatch_tool_call(tool_call, registry).await;
                        let (id, content) = match result {
                            Some(v) => v,
                            None => continue,
                        };
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": id,
                            "content": content
                        }));
                    }
                } else if let Some(content) = message_obj.get("content").and_then(|v| v.as_str()) {
                    println!("{content}");
                    return Ok(());
                } else {
                    eprintln!(
                        "Received message without content or tool_calls: {:#}",
                        message_obj
                    );
                }
            } else {
                eprintln!("Choice without message: {:#}", choice);
            }
        }
    }
}

async fn dispatch_tool_call(
    tool_call: &Value,
    registry: &ToolRegistry,
) -> Option<(String, String)> {
    let id = match tool_call.get("id").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            eprintln!("tool_call missing 'id': {:#}", tool_call);
            return None;
        }
    };

    let function_obj = match tool_call.get("function") {
        Some(f) => f,
        None => {
            eprintln!("tool_call missing 'function' object: {:#}", tool_call);
            return None;
        }
    };

    let function_name = match function_obj.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            eprintln!("function object missing 'name': {:#}", function_obj);
            return None;
        }
    };

    let args_str = match function_obj.get("arguments").and_then(|v| v.as_str()) {
        Some(a) => a,
        None => {
            eprintln!(
                "function object missing 'arguments' string: {:#}",
                function_obj
            );
            return None;
        }
    };

    let args: Value = match serde_json::from_str(args_str) {
        Ok(v) => v,
        Err(err) => {
            eprintln!(
                "failed to parse tool arguments: {} -- raw: {}",
                err, args_str
            );
            return None;
        }
    };

    let content = match registry.execute(function_name, &args).await {
        Ok(output) => output,
        Err(err) => err,
    };

    Some((id, content))
}

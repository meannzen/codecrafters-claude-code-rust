use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use serde_json::{Value, json};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    process,
};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let base_url = env::var("OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        eprintln!("OPENROUTER_API_KEY is not set");
        process::exit(1);
    });

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);

    let client = Client::with_config(config);

    let mut messages = vec![json!({"role": "user","content": args.prompt})];

    loop {
        let response: Value = client
            .chat()
            .create_byot(json!({
                             "messages": messages,
                             "model": "anthropic/claude-haiku-4.5",
                             "tools": [{
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
                            },
                           {
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
              }
             ]
             }
            ))
            .await?;

        if let Some(choices) = response.get("choices").and_then(|v| v.as_array()) {
            for choice in choices {
                if let Some(message_obj) = choice.get("message") {
                    messages.push(message_obj.clone());
                    if let Some(tool_calls) =
                        message_obj.get("tool_calls").and_then(|v| v.as_array())
                    {
                        for tool_call in tool_calls {
                            if let Some(id) = tool_call.get("id").and_then(|v| v.as_str())
                                && let Some(function) =
                                    tool_call.get("function").and_then(|v| v.as_object())
                                && let Some(args) =
                                    function.get("arguments").and_then(|s| s.as_str())
                                && let Some(functionn_name) =
                                    function.get("name").and_then(|s| s.as_str())
                            {
                                let args: Value = serde_json::from_str(args)?;
                                if let Some(file_path) =
                                    args.get("file_path").and_then(|o| o.as_str())
                                {
                                    if functionn_name == "Read" {
                                        let contents = fs::read_to_string(file_path)?;
                                        messages.push(json!({
                                            "role": "tool",
                                            "tool_call_id": id,
                                            "content": contents
                                        }));
                                    } else if functionn_name == "Write"
                                        && let Some(content) =
                                            args.get("content").and_then(|s| s.as_str())
                                    {
                                        let mut file = OpenOptions::new()
                                            .write(true)
                                            .create(true)
                                            .truncate(true)
                                            .open(file_path)?;

                                        file.write_all(content.as_bytes())?;

                                        messages.push(json!({
                                            "role": "tool",
                                            "tool_call_id": id,
                                            "content": "File written successfully"
                                        }));
                                    }
                                }
                            }
                        }
                    } else if let Some(contents) =
                        message_obj.get("content").and_then(|s| s.as_str())
                    {
                        println!("{contents}");
                        return Ok(());
                    }
                }
            }
        }
    }
}

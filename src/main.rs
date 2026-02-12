use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use serde_json::{Value, json};
use std::{env, fs, process};

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

    #[allow(unused_variables)]
    let response: Value = client
        .chat()
        .create_byot(json!({
            "messages": [
                {
                    "role": "user",
                    "content": args.prompt
                }
            ],
            "model": "anthropic/claude-haiku-4.5",
            "tools": [
                {
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
           }]
        }))
        .await?;

    eprintln!("Logs from your program will appear here!");

    if let Some(choices) = response.get("choices").and_then(|v| v.as_array()) {
        if let Some(message_obj) = choices
            .get(0)
            .and_then(|o| o.get("message"))
            .and_then(|v| v.as_object())
        {
            if let Some(tool_calls) = message_obj.get("tool_calls").and_then(|v| v.as_array())
                && tool_calls.len() > 0
            {
                if let Some(function) = tool_calls
                    .get(0)
                    .and_then(|o| o.get("function"))
                    .and_then(|v| v.as_object())
                {
                    if let Some(args) = function.get("arguments").and_then(|s| s.as_str()) {
                        let args: Value = serde_json::from_str(args)?;
                        if let Some(file_path) = args.get("file_path").and_then(|o| o.as_str()) {
                            let contents = fs::read_to_string(file_path)?;
                            println!("{contents}");
                        }
                    }
                }
            } else if let Some(contents) = message_obj.get("content").and_then(|s| s.as_str()) {
                println!("{contents}")
            }
        }
    }

    Ok(())
}

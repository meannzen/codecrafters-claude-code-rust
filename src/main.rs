mod agent;
mod skill;
mod tools;

use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use std::{env, process};

use tools::{ToolRegistry, bash::BashTool, read::ReadTool, write::WriteTool};

use crate::agent::Agent;
use crate::skill::SkillParser;

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let skills = SkillParser::parse(".claude/skills").unwrap_or_default();
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

    let registry = ToolRegistry::new(vec![
        Box::new(ReadTool),
        Box::new(WriteTool),
        Box::new(BashTool),
    ]);

    let prompt = SkillParser::resolve_prompt(&args.prompt, &skills).unwrap_or_else(|err| {
        eprintln!("{err}");
        process::exit(1);
    });

    let agent = Agent::builder()
        .client(&client)
        .skills(skills)
        .registry(&registry)
        .build();

    agent.run(&prompt).await
}

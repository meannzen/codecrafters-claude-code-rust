use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use std::{env, process};

use codecrafters_claude_code::{
    agent::Agent,
    lsp_client,
    skill::SkillParser,
    tools::{
        ToolRegistry, bash::BashTool, lsp::LspTool, read::ReadTool, skill::SkillTool,
        write::WriteTool,
    },
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

    let mut registry = ToolRegistry::new(vec![
        Box::new(ReadTool),
        Box::new(WriteTool),
        Box::new(BashTool),
        Box::new(SkillTool::new(skills.clone())),
    ]);

    match lsp_client::LspClient::from_environment().await {
        Ok(lsp_client) => {
            registry.add_tool(LspTool::new(lsp_client));
        }
        Err(e) => {
            eprintln!("{:?}", e);
        }
    };

    let prompt =
        SkillParser::resolve_prompt_multiple_skills(&args.prompt, &skills).unwrap_or_else(|err| {
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

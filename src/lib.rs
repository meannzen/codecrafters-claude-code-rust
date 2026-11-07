pub mod agent;
pub mod lsp_client;
pub mod skill;
pub mod tools;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

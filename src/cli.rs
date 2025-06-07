use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "airy", version, about, long_about = None)]
pub struct Cli {
    /// OpenRouter API Key.
    #[arg(long, env)]
    pub openrouter_api_key: Option<String>,

    /// OpenRouter Base URL.
    #[arg(long, env, default_value = "https://openrouter.ai/api/v1")]
    pub openrouter_base_url: String,

    /// OpenRouter model ID.
    #[arg(long, env, default_value = "mistralai/devstral-small:free")]
    pub model: String,

    /// Max tokens.
    #[arg(long, default_value_t = 1024)]
    pub max_tokens: i64,

    /// MySQL URL.
    #[arg(long, env)]
    pub mysql_url: Option<String>,

    /// PostgreSQL URL.
    #[arg(long, env)]
    pub postgres_url: Option<String>,

    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Subcommand, Debug)]
pub enum CliCommand {
    Chat {
        /// Set or get System prompt.
        #[arg(long, default_missing_value = "", num_args = 0..=1)]
        system_prompt: Option<String>,
    },
    Mcp {
        /// Run as a server using SSE.
        #[arg(long)]
        sse: bool,

        /// Set the Port.
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
}

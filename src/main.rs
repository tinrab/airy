use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use clap::Parser;
use rmcp::{
    ServiceExt,
    transport::{sse_server::SseServer, stdio},
};
use sqlx::{PgPool, mysql::MySqlPoolOptions};

use airy::{
    cli::{Cli, CliCommand},
    client::Client,
    error::{AppError, AppResult},
    repl::ReplSession,
    tool::{mysql::MySqlManager, postgres::PostgresManager},
};

#[tokio::main]
async fn main() -> AppResult<()> {
    let args = Cli::parse();

    macro_rules! impl_db {
        (
            $prompt_file:expr,
            $manager_type:ty,
            $pool:expr
            $(,)?
        ) => {
            let mut system_prompt = include_str!($prompt_file).to_string();
            if let CliCommand::Chat {
                system_prompt: Some(system_prompt_option),
            } = &args.command
            {
                if system_prompt_option.is_empty() {
                    println!("{}", system_prompt);
                    return Ok(());
                }
                system_prompt = system_prompt_option.clone();
            }

            let manager = <$manager_type>::new($pool, system_prompt);

            match args.command {
                CliCommand::Chat { .. } => {
                    let mut client = Client::create(
                        args.openrouter_base_url.clone(),
                        args.openrouter_api_key
                            .clone()
                            .ok_or(AppError::MissingApiKey)?,
                    )?;
                    client.add_tool(<$manager_type>::get_database_schema_tool_attr());
                    client.add_tool(<$manager_type>::execute_query_tool_attr());

                    let mut repl_session = ReplSession::new(client, Arc::new(manager), &args);
                    repl_session.run().await?;
                }
                CliCommand::Mcp { sse, port } => {
                    if sse {
                        let ct =
                            SseServer::serve(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port))
                                .await?
                                .with_service(move || manager.clone());
                        tokio::signal::ctrl_c().await?;
                        ct.cancel();
                    } else {
                        let service = manager.serve(stdio()).await.unwrap();
                        service.waiting().await.unwrap();
                    }
                }
            }
        };
    }

    if let Some(mysql_url) = args.mysql_url.as_ref() {
        let pool = MySqlPoolOptions::new().connect(mysql_url).await?;
        impl_db!("mysql_system_prompt.txt", MySqlManager, pool);
    } else if let Some(postgres_url) = args.postgres_url.as_ref() {
        let pool = PgPool::connect(postgres_url).await?;
        impl_db!("postgres_system_prompt.txt", PostgresManager, pool);
    } else {
        return Err(AppError::MissingDatabaseUrl);
    }

    Ok(())
}

use async_trait::async_trait;
use rmcp::{
    Error as McpError, ServerHandler,
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    tool,
};
use sqlx::mysql::MySqlPool;

use crate::{
    error::AppResult,
    mysql_utility::from_row,
    tool::{ExecuteQueryParams, Manager},
};

#[derive(Clone)]
pub struct MySqlManager {
    system_prompt: String,
    pool: MySqlPool,
}

impl MySqlManager {
    pub fn new(pool: MySqlPool, system_prompt: String) -> Self {
        Self {
            pool,
            system_prompt,
        }
    }
}

#[tool(tool_box)]
impl MySqlManager {
    #[tool(
        name = "mysqlGetDatabaseSchema",
        description = "Retrieves the schema (tables and columns)."
    )]
    pub async fn get_database_schema(&self) -> Result<CallToolResult, McpError> {
        Ok(Manager::get_database_schema(self).await?)
    }

    #[tool(
        name = "mysqlExecuteQuery",
        description = "Executes a SQL SELECT query and returns the results."
    )]
    pub async fn execute_query(
        &self,
        #[tool(aggr)] params: ExecuteQueryParams,
    ) -> Result<CallToolResult, McpError> {
        Ok(Manager::execute_query(self, params).await?)
    }
}

#[async_trait]
impl Manager for MySqlManager {
    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    async fn get_database_schema(&self) -> AppResult<CallToolResult> {
        let (database_name,): (String,) = sqlx::query_as("SELECT DATABASE()")
            .fetch_one(&self.pool)
            .await?;

        let tables: Vec<(Vec<u8>,)> = sqlx::query_as(
            "
                SELECT table_name
                FROM information_schema.tables
                WHERE table_schema = ? AND table_type = 'BASE TABLE'
            ",
        )
        .bind(&database_name)
        .fetch_all(&self.pool)
        .await?;

        if tables.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No tables found in database '{}'.",
                database_name,
            ))]));
        }

        let mut schema_desc = format!("Schema for database '{}':\n\n", database_name);

        for (table_name_bin,) in tables {
            let table_name = if let Ok(table_name) = String::from_utf8(table_name_bin) {
                table_name
            } else {
                continue;
            };

            schema_desc += &format!("Table: {}\n", table_name);

            let columns_rows: Vec<(
                String,
                Vec<u8>,
                String,
                Vec<u8>,
                Option<String>,
                Option<String>,
            )> = sqlx::query_as(
                "
                    SELECT column_name, column_type, is_nullable, column_key, column_default, extra
                    FROM information_schema.columns
                    WHERE table_schema = ? AND table_name = ?
                    ORDER BY ordinal_position
                ",
            )
            .bind(&database_name)
            .bind(&table_name)
            .fetch_all(&self.pool)
            .await?;

            for (column_name, column_type, is_nullable, column_key, column_default, extra) in
                columns_rows
            {
                let mut col_desc = format!(
                    "  - {}: {} ",
                    column_name,
                    String::from_utf8_lossy(&column_type),
                );
                if is_nullable == "NO" {
                    col_desc += "| NOT NULL";
                }
                let key = String::from_utf8_lossy(&column_key);
                if !key.is_empty() {
                    col_desc += &format!("| KEY: {}", key);
                }
                if let Some(column_default) = column_default.filter(|s| !s.is_empty()) {
                    col_desc += &format!("| DEFAULT: {}", column_default);
                }
                if let Some(extra) = extra.filter(|s| !s.is_empty()) {
                    col_desc += &format!("| EXTRA: {}", extra);
                }
                schema_desc += &col_desc;
                schema_desc += "\n";
            }

            schema_desc += "\n\n";
        }

        Ok(CallToolResult::success(vec![Content::text(schema_desc)]))
    }

    async fn execute_query(&self, params: ExecuteQueryParams) -> AppResult<CallToolResult> {
        let query = params.query.trim();
        // if !query.starts_with("SELECT") {
        //     return Ok(CallToolResult::error(vec![Content::text(
        //         "Error: Only SELECT queries are allowed with this tool for safety.",
        //     )]));
        // }

        let mut rows: Vec<serde_json::Value> = Vec::new();
        for row in sqlx::query(query).fetch_all(&self.pool).await? {
            rows.push(from_row(row)?);
        }
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&rows)?,
        )]))
    }
}

#[tool(tool_box)]
impl ServerHandler for MySqlManager {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                // .enable_prompts()
                // .enable_resources()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(self.system_prompt().into()),
        }
    }
}

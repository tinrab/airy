use std::sync::Arc;

use async_trait::async_trait;
use rmcp::{
    model::CallToolResult,
    schemars::{self, JsonSchema},
};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

pub mod mysql;
pub mod postgres;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ExecuteQueryParams {
    #[schemars(description = "The SQL SELECT query to execute.")]
    query: String,
}

#[async_trait]
pub trait Manager {
    fn system_prompt(&self) -> &str;

    async fn get_database_schema(&self) -> AppResult<CallToolResult>;

    async fn execute_query(&self, params: ExecuteQueryParams) -> AppResult<CallToolResult>;
}

pub type ManagerArc = Arc<dyn Manager + Send + Sync>;

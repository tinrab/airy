pub use openai_api_rs::{
    realtime::types::{FunctionType, ToolChoice},
    v1::{
        chat_completion::{
            ChatCompletionMessage, ChatCompletionRequest, ChatCompletionResponse, Content,
            MessageRole, Tool, ToolChoiceType, ToolType,
        },
        types::{Function, FunctionParameters, JSONSchemaType},
    },
};
use reqwest::{
    Client as RequestClient,
    header::{HeaderMap, HeaderValue},
};
use rmcp::model::Tool as McpTool;

use crate::error::AppResult;

pub struct Client {
    base_url: String,
    client: RequestClient,
    tools: Vec<Tool>,
}

impl Client {
    pub fn create(base_url: String, api_key: String) -> AppResult<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", api_key.trim())).unwrap(),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self {
            base_url,
            client,
            tools: Vec::new(),
        })
    }

    pub fn add_tool(&mut self, tool: McpTool) {
        self.tools.push(Tool {
            r#type: ToolType::Function,
            function: Function {
                name: tool.name.to_string(),
                description: if tool.description.is_empty() {
                    None
                } else {
                    Some(tool.description.to_string())
                },
                // Not sure what's the correct way to handle "void" functions.
                parameters: {
                    if tool.input_schema.is_empty()
                        || tool.input_schema.get("title")
                            == Some(&serde_json::Value::String("EmptyObject".into()))
                    {
                        FunctionParameters {
                            schema_type: JSONSchemaType::Object,
                            properties: Some(Default::default()),
                            required: None,
                        }
                    } else {
                        serde_json::from_value(tool.schema_as_json_value()).unwrap()
                    }
                },
            },
        });
    }

    pub async fn chat_completion(
        &self,
        mut req: ChatCompletionRequest,
    ) -> AppResult<ChatCompletionResponse> {
        if !self.tools.is_empty() {
            req = req.tools(self.tools.clone());
        }

        // Intentionally not using OpenAI's client from `openai_api_rs` in case custom parsing is needed.
        // OpenRouter's models are not guaranteed to follow the same schema.
        let res = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&req)
            .send()
            .await?
            .error_for_status()?;

        let text = res.text().await.unwrap_or_else(|_| "".to_string());
        Ok(serde_json::from_str(&text)?)
    }
}

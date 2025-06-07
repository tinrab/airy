use std::io::{self, Write};

use owo_colors::{OwoColorize, Stream::Stdout};
use rmcp::model::{self, CallToolResult};

use crate::{
    cli::Cli,
    client::{
        ChatCompletionMessage, ChatCompletionRequest, Client, Content, MessageRole, ToolChoiceType,
    },
    error::AppResult,
    tool::ManagerArc,
    utility::fix_tool_call,
};

pub struct ReplSession {
    client: Client,
    manager: ManagerArc,
    history: Vec<ChatCompletionMessage>,
    model: String,
    max_tokens: i64,
}

impl ReplSession {
    pub fn new(client: Client, manager: ManagerArc, args: &Cli) -> Self {
        let history = vec![ChatCompletionMessage {
            role: MessageRole::system,
            content: Content::Text(manager.system_prompt().into()),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];

        Self {
            client,
            manager,
            history,
            model: args.model.clone(),
            max_tokens: args.max_tokens,
        }
    }

    pub async fn run(&mut self) -> AppResult<()> {
        println!("Database REPL started. Type 'exit' to quit.");

        loop {
            print!("User> ");
            io::stdout().flush()?;
            let mut user_input = String::new();
            io::stdin().read_line(&mut user_input)?;
            let user_input = user_input.trim();

            if user_input.eq_ignore_ascii_case("exit") {
                break;
            }
            if user_input.is_empty() {
                continue;
            }

            self.history.push(ChatCompletionMessage {
                role: MessageRole::user,
                content: Content::Text(user_input.to_string()),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });

            'chat_response: loop {
                match self
                    .client
                    .chat_completion(
                        ChatCompletionRequest::new(self.model.clone(), self.history.clone())
                            .tool_choice(ToolChoiceType::Auto)
                            .max_tokens(self.max_tokens),
                    )
                    .await
                {
                    Ok(response) => {
                        if let Some(choice) = response.choices.into_iter().next() {
                            let assistant_message = choice.message;

                            self.history.push(ChatCompletionMessage {
                                role: MessageRole::assistant,
                                content: assistant_message
                                    .content
                                    .clone()
                                    .map_or(Content::Text("".into()), Content::Text),
                                name: assistant_message.name.clone(),
                                tool_calls: assistant_message.tool_calls.as_ref().map(
                                    |tool_calls| {
                                        tool_calls.iter().cloned().map(fix_tool_call).collect()
                                    },
                                ),
                                tool_call_id: None,
                            });

                            if let Some(tool_calls) = assistant_message.tool_calls {
                                for (tool_call_id, function_name, params_json) in
                                    tool_calls.into_iter().filter_map(|tool| {
                                        let params: Option<serde_json::Value> = tool
                                            .function
                                            .arguments
                                            .and_then(|args| serde_json::from_str(&args).ok());
                                        Some((tool.id, tool.function.name?, params))
                                    })
                                {
                                    let tool_result = match function_name.as_str() {
                                        "mysqlGetDatabaseSchema" => {
                                            self.manager.get_database_schema().await
                                        }
                                        "mysqlExecuteQuery" => {
                                            if let Some(params_json) = params_json {
                                                match serde_json::from_value(params_json) {
                                                    Ok(params) => {
                                                        self.manager.execute_query(params).await
                                                    }
                                                    Err(err) => Err(err.into()),
                                                }
                                            } else {
                                                Ok(CallToolResult::error(vec![
                                                    model::Content::text(format!(
                                                        "Missing required parameter for {}",
                                                        function_name,
                                                    )),
                                                ]))
                                            }
                                        }
                                        _ => Ok(CallToolResult::error(vec![model::Content::text(
                                            format!("Unknown tool: {}", function_name),
                                        )])),
                                    };

                                    let result_content = match tool_result {
                                        Ok(result) => {
                                            // Assuming CallToolResult content is a Vec<Content>, and the first one is text.
                                            if let Some(text) = result
                                                .content
                                                .first()
                                                .and_then(|content| content.as_text())
                                            {
                                                text.text.clone()
                                            } else if result.is_error.unwrap_or(false) {
                                                format!(
                                                    "Tool error {}: {:?}",
                                                    function_name, result.content,
                                                )
                                            } else {
                                                "Tool returned non-text or empty content".into()
                                            }
                                        }
                                        Err(err) => {
                                            format!(
                                                "Error executing tool {}: {}",
                                                function_name, err
                                            )
                                        }
                                    };

                                    self.history.push(ChatCompletionMessage {
                                        role: MessageRole::tool,
                                        tool_call_id: Some(tool_call_id),
                                        name: Some(function_name),
                                        content: Content::Text(result_content),
                                        tool_calls: None,
                                    });
                                }

                                continue 'chat_response;
                            } else if let Some(text) = assistant_message.content {
                                println!(
                                    "{}",
                                    format!("Assistant> {}", text)
                                        .if_supports_color(Stdout, |text| text.blue())
                                );
                            }
                        }

                        break 'chat_response;
                    }
                    Err(err) => {
                        println!(
                            "{}",
                            format!("ERROR: {}", err).if_supports_color(Stdout, |text| text.red())
                        );
                        self.history.pop();
                        break 'chat_response;
                    }
                }
            }
        }
        Ok(())
    }
}

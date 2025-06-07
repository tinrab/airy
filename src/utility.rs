use openai_api_rs::v1::chat_completion::ToolCall;

/// Fixes the tool call schema.
/// Not doing this will cause "Internal Server Error" for some models.
pub fn fix_tool_call(mut tool_call: ToolCall) -> ToolCall {
    if tool_call.function.arguments.is_none() {
        tool_call.function.arguments = Some("{}".into());
    }
    tool_call
}

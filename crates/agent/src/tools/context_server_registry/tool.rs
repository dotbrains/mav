use super::mcp_tool_id;
use crate::{AgentToolOutput, AnyAgentTool, ToolCallEventStream, ToolInput};
use agent_client_protocol::schema::v1 as acp;
use anyhow::Result;
use context_server::ContextServerId;
use futures::FutureExt as _;
use gpui::{App, AppContext, Entity, SharedString, Task};
use language_model::{LanguageModelImage, LanguageModelImageExt, LanguageModelToolResultContent};
use project::context_server_store::ContextServerStore;
use std::sync::Arc;

pub(super) struct ContextServerTool {
    store: Entity<ContextServerStore>,
    server_id: ContextServerId,
    tool: context_server::types::Tool,
}

impl ContextServerTool {
    pub(super) fn new(
        store: Entity<ContextServerStore>,
        server_id: ContextServerId,
        tool: context_server::types::Tool,
    ) -> Self {
        Self {
            store,
            server_id,
            tool,
        }
    }
}

impl AnyAgentTool for ContextServerTool {
    fn name(&self) -> SharedString {
        self.tool.name.clone().into()
    }

    fn description(&self) -> SharedString {
        self.tool.description.clone().unwrap_or_default().into()
    }

    fn kind(&self) -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(&self, _input: serde_json::Value, _cx: &mut App) -> SharedString {
        format!("Run MCP tool `{}`", self.tool.name).into()
    }

    fn input_schema(
        &self,
        format: language_model::LanguageModelToolSchemaFormat,
    ) -> Result<serde_json::Value> {
        let mut schema = self.tool.input_schema.clone();
        language_model::tool_schema::adapt_schema_to_format(&mut schema, format)?;
        Ok(match schema {
            serde_json::Value::Null => {
                serde_json::json!({ "type": "object", "properties": [] })
            }
            serde_json::Value::Object(map) if map.is_empty() => {
                serde_json::json!({ "type": "object", "properties": [] })
            }
            _ => schema,
        })
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<serde_json::Value>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<AgentToolOutput, AgentToolOutput>> {
        let Some(server) = self.store.read(cx).get_running_server(&self.server_id) else {
            return Task::ready(Err(anyhow::anyhow!("Context server not found").into()));
        };
        let tool_name = self.tool.name.clone();
        let tool_id = mcp_tool_id(&self.server_id.0, &self.tool.name);
        let display_name = self.tool.name.clone();
        let initial_title = self.initial_title(serde_json::Value::Null, cx);
        let authorize =
            event_stream.authorize_third_party_tool(initial_title, tool_id, display_name, cx);

        cx.spawn(async move |cx| {
            let input = input
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;

            authorize
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;

            let Some(protocol) = server.client() else {
                return Err(anyhow::anyhow!("Context server not initialized").into());
            };

            let arguments = if let serde_json::Value::Object(map) = input {
                Some(map.into_iter().collect())
            } else {
                None
            };

            log::trace!(
                "Running tool: {} with arguments: {:?}",
                tool_name,
                arguments
            );

            let request = protocol.request::<context_server::types::requests::CallTool>(
                context_server::types::CallToolParams {
                    name: tool_name,
                    arguments,
                    meta: None,
                },
            );

            let response = futures::select! {
                response = request.fuse() => response?,
                _ = event_stream.cancelled_by_user().fuse() => {
                    return Err(anyhow::anyhow!("MCP tool cancelled by user").into());
                }
            };

            if response.is_error == Some(true) {
                let error_message: String =
                    response.content.iter().filter_map(|c| c.text()).collect();
                return Err(anyhow::anyhow!(error_message).into());
            }

            let mut llm_output = Vec::new();
            let mut tool_call_content = Vec::new();
            let mut concatenated_text = String::new();
            for content in response.content {
                match content {
                    context_server::types::ToolResponseContent::Text { text } => {
                        concatenated_text.push_str(&text);
                        tool_call_content.push(acp::ToolCallContent::Content(acp::Content::new(
                            acp::ContentBlock::Text(acp::TextContent::new(text.clone())),
                        )));
                        llm_output.push(LanguageModelToolResultContent::Text(text.into()));
                    }
                    context_server::types::ToolResponseContent::Image { data, mime_type } => {
                        tool_call_content.push(acp::ToolCallContent::Content(acp::Content::new(
                            acp::ContentBlock::Image(acp::ImageContent::new(
                                data.clone(),
                                mime_type.clone(),
                            )),
                        )));
                        let language_model_image = cx
                            .background_spawn({
                                let mime_type = mime_type.clone();
                                async move {
                                    LanguageModelImage::from_base64_image(&data, &mime_type)
                                }
                            })
                            .await;
                        match language_model_image {
                            Ok(Some(image)) => {
                                llm_output.push(LanguageModelToolResultContent::Image(image));
                            }
                            Ok(None) => {
                                log::warn!(
                                    "Skipping MCP tool response image with MIME type `{}` because it cannot be converted for language model input",
                                    mime_type
                                );
                            }
                            Err(error) => {
                                log::warn!(
                                    "Failed to convert MCP tool response image with MIME type `{}` for language model input: {:#}",
                                    mime_type,
                                    error
                                );
                            }
                        }
                    }
                    context_server::types::ToolResponseContent::Audio { .. } => {
                        log::warn!("Ignoring audio content from tool response");
                    }
                    context_server::types::ToolResponseContent::Resource { .. } => {
                        log::warn!("Ignoring resource content from tool response");
                    }
                    context_server::types::ToolResponseContent::ResourceLink { .. } => {
                        log::warn!("Ignoring resource link content from tool response");
                    }
                }
            }
            if !tool_call_content.is_empty() {
                event_stream
                    .update_fields(acp::ToolCallUpdateFields::new().content(tool_call_content));
            }
            let raw_output = serde_json::Value::String(concatenated_text);
            Ok(AgentToolOutput {
                raw_output,
                llm_output,
            })
        })
    }

    fn replay(
        &self,
        _input: serde_json::Value,
        _output: serde_json::Value,
        _event_stream: ToolCallEventStream,
        _cx: &mut App,
    ) -> Result<()> {
        Ok(())
    }
}

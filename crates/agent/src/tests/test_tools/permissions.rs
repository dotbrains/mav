use super::*;

#[derive(JsonSchema, Serialize, Deserialize)]
pub struct ToolRequiringPermissionInput {}

pub struct ToolRequiringPermission;

impl AgentTool for ToolRequiringPermission {
    type Input = ToolRequiringPermissionInput;
    type Output = String;

    const NAME: &'static str = "tool_requiring_permission";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "This tool requires permission".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<String, String>> {
        cx.spawn(async move |cx| {
            let _input = input.recv().await.map_err(|e| e.to_string())?;

            let authorize = cx.update(|cx| {
                let context = crate::ToolPermissionContext::new(Self::NAME, vec![String::new()]);
                event_stream.authorize("Authorize?", context, cx)
            });
            authorize.await.map_err(|e| e.to_string())?;
            Ok("Allowed".to_string())
        })
    }
}

/// A second tool that also requires permission, used to verify that
/// permission decisions scoped to one tool don't leak into prompts for a
/// different tool.
#[derive(JsonSchema, Serialize, Deserialize)]
pub struct ToolRequiringPermission2Input {}

pub struct ToolRequiringPermission2;

impl AgentTool for ToolRequiringPermission2 {
    type Input = ToolRequiringPermission2Input;
    type Output = String;

    const NAME: &'static str = "tool_requiring_permission_2";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "This tool also requires permission".into()
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<String, String>> {
        cx.spawn(async move |cx| {
            let _input = input.recv().await.map_err(|e| e.to_string())?;

            let authorize = cx.update(|cx| {
                let context = crate::ToolPermissionContext::new(Self::NAME, vec![String::new()]);
                event_stream.authorize("Authorize?", context, cx)
            });
            authorize.await.map_err(|e| e.to_string())?;
            Ok("Allowed".to_string())
        })
    }
}

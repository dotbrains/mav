use super::*;

impl ToolCallEventStream {
    /// Prompts the user to choose between an explicit set of actions and
    /// returns the chosen `option_id`.
    ///
    /// Unlike [`Self::authorize`] / [`Self::authorize_always_prompt`], this
    /// does not interpret the user's choice as a permission grant — callers
    /// are responsible for handling each `option_id` explicitly. Use this
    /// when a tool needs the user to pick between several side-effecting
    /// actions (for example, "Save" vs "Discard" for a dirty buffer).
    pub fn prompt_for_decision(
        &self,
        title: Option<String>,
        message: Option<String>,
        options: Vec<acp::PermissionOption>,
        cx: &mut App,
    ) -> Task<Result<acp::PermissionOptionId>> {
        let options = acp_thread::PermissionOptions::Flat(options);
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        cx.spawn(async move |_cx| {
            let mut fields = acp::ToolCallUpdateFields::new();
            if let Some(title) = title {
                fields = fields.title(title);
            }
            if let Some(message) = message {
                fields = fields.content(vec![acp::ToolCallContent::from(message)]);
            }

            let (response_tx, response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        tool_call: acp::ToolCallUpdate::new(tool_use_id.to_string(), fields),
                        options,
                        response: response_tx,
                        context: None,
                        kind: acp_thread::AuthorizationKind::ActionChoice,
                    },
                )))
            {
                log::error!("Failed to send tool call decision prompt: {error}");
                return Err(anyhow!("Failed to send tool call decision prompt: {error}"));
            }

            let outcome = response_rx
                .await
                .map_err(|_| anyhow!("authorization channel closed"))?;
            Ok(outcome.option_id)
        })
    }
}

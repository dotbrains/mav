use super::*;

impl Thread {
    pub fn resume(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        self.messages.push(Arc::new(Message::Resume));
        cx.notify();

        log::debug!("Total messages in thread: {}", self.messages.len());
        self.run_turn(cx)
    }

    /// Sending a message results in the model streaming a response, which could include tool calls.
    /// After calling tools, the model will stops and waits for any outstanding tool calls to be completed and their results sent.
    /// The returned channel will report all the occurrences in which the model stops before erroring or ending its turn.
    pub fn send<T>(
        &mut self,
        id: ClientUserMessageId,
        content: impl IntoIterator<Item = T>,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>>
    where
        T: Into<UserMessageContent>,
    {
        let content = content.into_iter().map(Into::into).collect::<Arc<_>>();
        log::debug!("Thread::send content: {:?}", content);

        self.messages
            .push(Arc::new(Message::User(UserMessage { id, content })));
        cx.notify();

        self.send_existing(cx)
    }

    pub fn send_existing(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        let model = self
            .model()
            .ok_or_else(|| anyhow!(NoModelConfiguredError))?;

        log::info!("Thread::send called with model: {}", model.name().0);
        self.advance_prompt_id();

        log::debug!("Total messages in thread: {}", self.messages.len());
        self.run_turn(cx)
    }

    pub fn push_acp_user_block(
        &mut self,
        id: ClientUserMessageId,
        blocks: impl IntoIterator<Item = acp::ContentBlock>,
        path_style: PathStyle,
        cx: &mut Context<Self>,
    ) {
        let content = blocks
            .into_iter()
            .map(|block| UserMessageContent::from_content_block(block, path_style))
            .collect::<Arc<_>>();
        self.messages
            .push(Arc::new(Message::User(UserMessage { id, content })));
        cx.notify();
    }

    pub fn push_acp_agent_block(&mut self, block: acp::ContentBlock, cx: &mut Context<Self>) {
        let text = match block {
            acp::ContentBlock::Text(text_content) => text_content.text,
            acp::ContentBlock::Image(_) => "[image]".to_string(),
            acp::ContentBlock::Audio(_) => "[audio]".to_string(),
            acp::ContentBlock::ResourceLink(resource_link) => resource_link.uri,
            acp::ContentBlock::Resource(resource) => match resource.resource {
                acp::EmbeddedResourceResource::TextResourceContents(resource) => resource.uri,
                acp::EmbeddedResourceResource::BlobResourceContents(resource) => resource.uri,
                _ => "[resource]".to_string(),
            },
            _ => "[unknown]".to_string(),
        };

        self.messages.push(Arc::new(Message::Agent(AgentMessage {
            content: vec![AgentMessageContent::Text(text)],
            ..Default::default()
        })));
        cx.notify();
    }
}

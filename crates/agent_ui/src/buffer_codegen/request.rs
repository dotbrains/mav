use super::*;

impl CodegenAlternative {
    pub(super) fn build_request_tools(
        &self,
        model: &Arc<dyn LanguageModel>,
        user_prompt: String,
        context_task: Shared<Task<Option<LoadedContext>>>,
        cx: &mut App,
    ) -> Result<Task<LanguageModelRequest>> {
        let buffer = self.buffer.read(cx).snapshot(cx);
        let language = buffer.language_at(self.range.start);
        let language_name = if let Some(language) = language.as_ref() {
            if Arc::ptr_eq(language, &language::PLAIN_TEXT) {
                None
            } else {
                Some(language.name())
            }
        } else {
            None
        };

        let language_name = language_name.as_ref();
        let start = buffer.point_to_buffer_offset(self.range.start);
        let end = buffer.point_to_buffer_offset(self.range.end);
        let (buffer, range) = if let Some((start, end)) = start.zip(end) {
            let (start_buffer, start_buffer_offset) = start;
            let (end_buffer, end_buffer_offset) = end;
            if start_buffer.remote_id() == end_buffer.remote_id() {
                (start_buffer.clone(), start_buffer_offset..end_buffer_offset)
            } else {
                anyhow::bail!("invalid transformation range");
            }
        } else {
            anyhow::bail!("invalid transformation range");
        };

        let system_prompt = self
            .builder
            .generate_inline_transformation_prompt_tools(
                language_name,
                buffer,
                range.start.0..range.end.0,
            )
            .context("generating content prompt")?;

        let temperature = AgentSettings::temperature_for_model(model, cx);

        let tool_input_format = model.tool_input_format();
        let tool_choice = model
            .supports_tool_choice(LanguageModelToolChoice::Any)
            .then_some(LanguageModelToolChoice::Any);

        Ok(cx.spawn(async move |_cx| {
            let mut messages = vec![LanguageModelRequestMessage {
                role: Role::System,
                content: vec![system_prompt.into()],
                cache: false,
                reasoning_details: None,
            }];

            let mut user_message = LanguageModelRequestMessage {
                role: Role::User,
                content: Vec::new(),
                cache: false,
                reasoning_details: None,
            };

            if let Some(context) = context_task.await {
                context.add_to_request_message(&mut user_message);
            }

            user_message.content.push(user_prompt.into());
            messages.push(user_message);

            let tools = vec![
                LanguageModelRequestTool {
                    name: REWRITE_SECTION_TOOL_NAME.to_string(),
                    description: "Replaces text in <rewrite_this></rewrite_this> tags with your replacement_text.".to_string(),
                    input_schema: language_model::tool_schema::root_schema_for::<RewriteSectionInput>(tool_input_format).to_value(),
                    use_input_streaming: false,
                },
                LanguageModelRequestTool {
                    name: FAILURE_MESSAGE_TOOL_NAME.to_string(),
                    description: "Use this tool to provide a message to the user when you're unable to complete a task.".to_string(),
                    input_schema: language_model::tool_schema::root_schema_for::<FailureMessageInput>(tool_input_format).to_value(),
                    use_input_streaming: false,
                },
            ];

            LanguageModelRequest {
                thread_id: None,
                prompt_id: None,
                intent: Some(CompletionIntent::InlineAssist),
                tools,
                tool_choice,
                stop: Vec::new(),
                temperature,
                messages,
                thinking_allowed: false,
                thinking_effort: None,
                speed: None,
                compact_at_tokens: None,
            }
        }))
    }

    pub(super) fn build_request(
        &self,
        model: &Arc<dyn LanguageModel>,
        user_prompt: String,
        context_task: Shared<Task<Option<LoadedContext>>>,
        cx: &mut App,
    ) -> Result<Task<LanguageModelRequest>> {
        if Self::use_streaming_tools(model.as_ref(), cx) {
            return self.build_request_tools(model, user_prompt, context_task, cx);
        }

        let buffer = self.buffer.read(cx).snapshot(cx);
        let language = buffer.language_at(self.range.start);
        let language_name = if let Some(language) = language.as_ref() {
            if Arc::ptr_eq(language, &language::PLAIN_TEXT) {
                None
            } else {
                Some(language.name())
            }
        } else {
            None
        };

        let language_name = language_name.as_ref();
        let start = buffer.point_to_buffer_offset(self.range.start);
        let end = buffer.point_to_buffer_offset(self.range.end);
        let (buffer, range) = if let Some((start, end)) = start.zip(end) {
            let (start_buffer, start_buffer_offset) = start;
            let (end_buffer, end_buffer_offset) = end;
            if start_buffer.remote_id() == end_buffer.remote_id() {
                (start_buffer.clone(), start_buffer_offset..end_buffer_offset)
            } else {
                anyhow::bail!("invalid transformation range");
            }
        } else {
            anyhow::bail!("invalid transformation range");
        };

        let prompt = self
            .builder
            .generate_inline_transformation_prompt(
                user_prompt,
                language_name,
                buffer,
                range.start.0..range.end.0,
            )
            .context("generating content prompt")?;

        let temperature = AgentSettings::temperature_for_model(model, cx);

        Ok(cx.spawn(async move |_cx| {
            let mut request_message = LanguageModelRequestMessage {
                role: Role::User,
                content: Vec::new(),
                cache: false,
                reasoning_details: None,
            };

            if let Some(context) = context_task.await {
                context.add_to_request_message(&mut request_message);
            }

            request_message.content.push(prompt.into());

            LanguageModelRequest {
                thread_id: None,
                prompt_id: None,
                intent: Some(CompletionIntent::InlineAssist),
                tools: Vec::new(),
                tool_choice: None,
                stop: Vec::new(),
                temperature,
                messages: vec![request_message],
                thinking_allowed: false,
                thinking_effort: None,
                speed: None,
                compact_at_tokens: None,
            }
        }))
    }
}

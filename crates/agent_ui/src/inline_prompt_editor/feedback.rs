use super::*;

impl<T: 'static> PromptEditor<T> {
    pub(super) fn thumbs_up(
        &mut self,
        _: &ThumbsUpResult,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.session_state.completion {
            CompletionState::Pending => {
                self.toast("Can't rate, still generating...", None, cx);
                return;
            }
            CompletionState::Rated => {
                self.toast(
                    "Already rated this completion",
                    Some(self.session_state.session_id),
                    cx,
                );
                return;
            }
            CompletionState::Generated { completion_text } => {
                let model_info = self.model_selector.read(cx).active_model(cx);
                let (model_id, use_streaming_tools) = {
                    let Some(configured_model) = model_info else {
                        self.toast("No configured model", None, cx);
                        return;
                    };
                    (
                        configured_model.model.telemetry_id(),
                        CodegenAlternative::use_streaming_tools(
                            configured_model.model.as_ref(),
                            cx,
                        ),
                    )
                };

                let selected_text = match &self.mode {
                    PromptEditorMode::Buffer { codegen, .. } => {
                        codegen.read(cx).selected_text(cx).map(|s| s.to_string())
                    }
                    PromptEditorMode::Terminal { .. } => None,
                };

                let prompt = self.editor.read(cx).text(cx);

                let kind = match &self.mode {
                    PromptEditorMode::Buffer { .. } => "inline",
                    PromptEditorMode::Terminal { .. } => "inline_terminal",
                };

                telemetry::event!(
                    "Inline Assistant Rated",
                    rating = "positive",
                    session_id = self.session_state.session_id.to_string(),
                    kind = kind,
                    model = model_id,
                    prompt = prompt,
                    completion = completion_text,
                    selected_text = selected_text,
                    use_streaming_tools
                );

                self.session_state.completion = CompletionState::Rated;

                cx.notify();
            }
        }
    }

    pub(super) fn thumbs_down(
        &mut self,
        _: &ThumbsDownResult,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.session_state.completion {
            CompletionState::Pending => {
                self.toast("Can't rate, still generating...", None, cx);
                return;
            }
            CompletionState::Rated => {
                self.toast(
                    "Already rated this completion",
                    Some(self.session_state.session_id),
                    cx,
                );
                return;
            }
            CompletionState::Generated { completion_text } => {
                let model_info = self.model_selector.read(cx).active_model(cx);
                let (model_telemetry_id, use_streaming_tools) = {
                    let Some(configured_model) = model_info else {
                        self.toast("No configured model", None, cx);
                        return;
                    };
                    (
                        configured_model.model.telemetry_id(),
                        CodegenAlternative::use_streaming_tools(
                            configured_model.model.as_ref(),
                            cx,
                        ),
                    )
                };

                let selected_text = match &self.mode {
                    PromptEditorMode::Buffer { codegen, .. } => {
                        codegen.read(cx).selected_text(cx).map(|s| s.to_string())
                    }
                    PromptEditorMode::Terminal { .. } => None,
                };

                let prompt = self.editor.read(cx).text(cx);

                let kind = match &self.mode {
                    PromptEditorMode::Buffer { .. } => "inline",
                    PromptEditorMode::Terminal { .. } => "inline_terminal",
                };

                telemetry::event!(
                    "Inline Assistant Rated",
                    rating = "negative",
                    session_id = self.session_state.session_id.to_string(),
                    kind = kind,
                    model = model_telemetry_id,
                    prompt = prompt,
                    completion = completion_text,
                    selected_text = selected_text,
                    use_streaming_tools
                );

                self.session_state.completion = CompletionState::Rated;

                cx.notify();
            }
        }
    }

    fn toast(&mut self, msg: &str, uuid: Option<Uuid>, cx: &mut Context<'_, PromptEditor<T>>) {
        self.workspace
            .update(cx, |workspace, cx| {
                enum InlinePromptRating {}
                workspace.show_toast(
                    {
                        let mut toast = Toast::new(
                            NotificationId::unique::<InlinePromptRating>(),
                            msg.to_string(),
                        )
                        .autohide();

                        if let Some(uuid) = uuid {
                            toast = toast.on_click("Click to copy rating ID", move |_, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(uuid.to_string()));
                            });
                        };

                        toast
                    },
                    cx,
                );
            })
            .ok();
    }
}

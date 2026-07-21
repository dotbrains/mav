use super::*;

impl Thread {
    pub fn model(&self) -> Option<&Arc<dyn LanguageModel>> {
        self.model.as_model()
    }

    pub(crate) fn ensure_model(
        &mut self,
        default_model: Option<&Arc<dyn LanguageModel>>,
        cx: &mut Context<Self>,
    ) {
        let resolved = match &self.model {
            ThreadModel::Ready(_) => return,
            ThreadModel::Unresolved(selection) => {
                LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                    registry
                        .select_model(selection, cx)
                        .map(|configured| configured.model)
                })
            }
            ThreadModel::Unset => default_model.cloned(),
        };

        if let Some(model) = resolved {
            self.set_model(model, cx);
        }
    }

    pub fn set_model(&mut self, model: Arc<dyn LanguageModel>, cx: &mut Context<Self>) {
        let old_usage = self.latest_token_usage();
        self.model = ThreadModel::Ready(model.clone());
        let new_caps = Self::prompt_capabilities(self.model.as_model().map(|model| model.as_ref()));
        let new_usage = self.latest_token_usage();
        if old_usage != new_usage {
            cx.emit(TokenUsageUpdated(new_usage));
        }
        self.prompt_capabilities_tx.send(new_caps).log_err();

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_model(model.clone(), cx);
                    }
                })
                .ok();
        }

        cx.notify()
    }

    pub fn summarization_model(&self) -> Option<&Arc<dyn LanguageModel>> {
        self.summarization_model.as_ref()
    }

    pub fn set_summarization_model(
        &mut self,
        model: Option<Arc<dyn LanguageModel>>,
        cx: &mut Context<Self>,
    ) {
        self.summarization_model = model.clone();

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    thread.set_summarization_model(model.clone(), cx)
                })
                .ok();
        }
        cx.notify()
    }

    pub fn thinking_enabled(&self) -> bool {
        self.thinking_enabled
    }

    pub fn set_thinking_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.thinking_enabled = enabled;

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_thinking_enabled(enabled, cx);
                    }
                })
                .ok();
        }
        cx.notify();
    }

    pub fn thinking_effort(&self) -> Option<&String> {
        self.thinking_effort.as_ref()
    }

    pub fn set_thinking_effort(&mut self, effort: Option<String>, cx: &mut Context<Self>) {
        self.thinking_effort = effort.clone();

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_thinking_effort(effort.clone(), cx)
                    }
                })
                .ok();
        }
        cx.notify();
    }

    pub fn speed(&self) -> Option<Speed> {
        self.speed
    }

    pub fn set_speed(&mut self, speed: Speed, cx: &mut Context<Self>) {
        self.speed = Some(speed);

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_speed(speed, cx);
                    }
                })
                .ok();
        }
        cx.notify();
    }
}

use super::*;

impl Thread {
    pub fn title(&self) -> Option<SharedString> {
        self.title.clone()
    }

    pub fn is_generating_summary(&self) -> bool {
        self.pending_summary_generation.is_some()
    }

    pub fn is_generating_title(&self) -> bool {
        self.pending_title_generation.is_some()
    }

    pub fn has_failed_title_generation(&self) -> bool {
        self.title_generation_failed
    }

    pub fn can_generate_title(&self) -> bool {
        self.pending_title_generation.is_none() && self.summarization_model.is_some()
    }

    pub fn summary(&mut self, cx: &mut Context<Self>) -> Shared<Task<Option<SharedString>>> {
        if let Some(summary) = self.summary.as_ref() {
            return Task::ready(Some(summary.clone())).shared();
        }
        if let Some(task) = self.pending_summary_generation.clone() {
            return task;
        }
        let Some(model) = self.summarization_model.clone() else {
            log::error!("No summarization model available");
            return Task::ready(None).shared();
        };
        let mut request = LanguageModelRequest {
            intent: Some(CompletionIntent::ThreadContextSummarization),
            temperature: AgentSettings::temperature_for_model(&model, cx),
            ..Default::default()
        };

        self.extend_request_history_until(&mut request.messages, self.messages.len());

        request.messages.push(LanguageModelRequestMessage {
            role: Role::User,
            content: vec![SUMMARIZE_THREAD_DETAILED_PROMPT.into()],
            cache: false,
            reasoning_details: None,
        });

        let task = cx
            .spawn(async move |this, cx| {
                let mut summary = String::new();
                let mut messages = model.stream_completion(request, cx).await.log_err()?;
                while let Some(event) = messages.next().await {
                    let event = event.log_err()?;
                    let text = match event {
                        LanguageModelCompletionEvent::Text(text) => text,
                        _ => continue,
                    };

                    let mut lines = text.lines();
                    summary.extend(lines.next());
                }

                log::debug!("Setting summary: {}", summary);
                let summary = SharedString::from(summary);

                this.update(cx, |this, cx| {
                    this.summary = Some(summary.clone());
                    this.pending_summary_generation = None;
                    cx.notify()
                })
                .ok()?;

                Some(summary)
            })
            .shared();
        self.pending_summary_generation = Some(task.clone());
        task
    }

    pub fn generate_title(&mut self, cx: &mut Context<Self>) {
        if !self.can_generate_title() {
            return;
        }
        let Some(model) = self.summarization_model.clone() else {
            return;
        };
        self.spawn_title_generation(model, None, cx);
    }

    pub fn regenerate_title(&mut self, cx: &mut Context<Self>) -> bool {
        self.regenerate_title_with_callback(cx, |_title, _cx| {})
    }

    pub fn regenerate_title_with_callback(
        &mut self,
        cx: &mut Context<Self>,
        on_generated_title: impl FnOnce(SharedString, &mut Context<Self>) + 'static,
    ) -> bool {
        if self.pending_title_generation.is_some() {
            return false;
        }

        let Some(model) = self.summarization_model.clone() else {
            return false;
        };

        self.spawn_title_generation(model, Some(Box::new(on_generated_title)), cx);

        true
    }

    fn spawn_title_generation(
        &mut self,
        model: Arc<dyn LanguageModel>,
        on_generated_title: Option<Box<dyn FnOnce(SharedString, &mut Context<Self>)>>,
        cx: &mut Context<Self>,
    ) {
        self.title_generation_failed = false;
        log::debug!("Generating title with model: {:?}", model.name());

        let temperature = AgentSettings::temperature_for_model(&model, cx);
        let request = build_thread_title_request(&self.messages, temperature);

        let title_generation = cx.spawn(async move |_this, cx| {
            stream_thread_title(model, request, cx)
                .await
                .context("failed to generate thread title")
                .map(SharedString::from)
                .log_err()
        });

        self.pending_title_generation = Some(cx.spawn(async move |this, cx| {
            let title = title_generation.await;
            _ = this.update(cx, |this, cx| {
                this.pending_title_generation = None;
                if let Some(title) = title {
                    this.set_title(title.clone(), cx);
                    if let Some(on_generated_title) = on_generated_title {
                        on_generated_title(title, cx);
                    }
                } else {
                    this.title_generation_failed = true;
                    cx.emit(TitleUpdated);
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub fn set_title(&mut self, title: SharedString, cx: &mut Context<Self>) {
        self.pending_title_generation = None;
        self.title_generation_failed = false;
        if Some(&title) != self.title.as_ref() {
            self.title = Some(title);
            cx.emit(TitleUpdated);
            cx.notify();
        }
    }

    pub(super) fn clear_summary(&mut self) {
        self.summary = None;
        self.pending_summary_generation = None;
    }
}

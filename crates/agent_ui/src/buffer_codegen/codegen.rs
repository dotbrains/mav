use super::*;

impl BufferCodegen {
    pub fn new(
        buffer: Entity<MultiBuffer>,
        range: Range<Anchor>,
        initial_transaction_id: Option<TransactionId>,
        session_id: Uuid,
        builder: Arc<PromptBuilder>,
        cx: &mut Context<Self>,
    ) -> Self {
        let codegen = cx.new(|cx| {
            CodegenAlternative::new(
                buffer.clone(),
                range.clone(),
                false,
                builder.clone(),
                session_id,
                cx,
            )
        });
        let mut this = Self {
            is_insertion: range.to_offset(&buffer.read(cx).snapshot(cx)).is_empty(),
            alternatives: vec![codegen],
            active_alternative: 0,
            seen_alternatives: HashSet::default(),
            subscriptions: Vec::new(),
            buffer,
            range,
            initial_transaction_id,
            builder,
            session_id,
        };
        this.activate(0, cx);
        this
    }

    fn subscribe_to_alternative(&mut self, cx: &mut Context<Self>) {
        let codegen = self.active_alternative().clone();
        self.subscriptions.clear();
        self.subscriptions
            .push(cx.observe(&codegen, |_, _, cx| cx.notify()));
        self.subscriptions
            .push(cx.subscribe(&codegen, |_, _, event, cx| cx.emit(*event)));
    }

    pub fn active_completion(&self, cx: &App) -> Option<String> {
        self.active_alternative().read(cx).current_completion()
    }

    pub fn active_alternative(&self) -> &Entity<CodegenAlternative> {
        &self.alternatives[self.active_alternative]
    }

    pub fn language_name(&self, cx: &App) -> Option<LanguageName> {
        self.active_alternative().read(cx).language_name(cx)
    }

    pub fn status<'a>(&self, cx: &'a App) -> &'a CodegenStatus {
        &self.active_alternative().read(cx).status
    }

    pub fn alternative_count(&self, cx: &App) -> usize {
        LanguageModelRegistry::read_global(cx)
            .inline_alternative_models()
            .len()
            + 1
    }

    pub fn cycle_prev(&mut self, cx: &mut Context<Self>) {
        let next_active_ix = if self.active_alternative == 0 {
            self.alternatives.len() - 1
        } else {
            self.active_alternative - 1
        };
        self.activate(next_active_ix, cx);
    }

    pub fn cycle_next(&mut self, cx: &mut Context<Self>) {
        let next_active_ix = (self.active_alternative + 1) % self.alternatives.len();
        self.activate(next_active_ix, cx);
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        self.active_alternative()
            .update(cx, |codegen, cx| codegen.set_active(false, cx));
        self.seen_alternatives.insert(index);
        self.active_alternative = index;
        self.active_alternative()
            .update(cx, |codegen, cx| codegen.set_active(true, cx));
        self.subscribe_to_alternative(cx);
        cx.notify();
    }

    pub fn start(
        &mut self,
        primary_model: Arc<dyn LanguageModel>,
        user_prompt: String,
        context_task: Shared<Task<Option<LoadedContext>>>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let alternative_models = LanguageModelRegistry::read_global(cx)
            .inline_alternative_models()
            .to_vec();

        self.active_alternative()
            .update(cx, |alternative, cx| alternative.undo(cx));
        self.activate(0, cx);
        self.alternatives.truncate(1);

        for _ in 0..alternative_models.len() {
            self.alternatives.push(cx.new(|cx| {
                CodegenAlternative::new(
                    self.buffer.clone(),
                    self.range.clone(),
                    false,
                    self.builder.clone(),
                    self.session_id,
                    cx,
                )
            }));
        }

        for (model, alternative) in iter::once(primary_model)
            .chain(alternative_models)
            .zip(&self.alternatives)
        {
            alternative.update(cx, |alternative, cx| {
                alternative.start(user_prompt.clone(), context_task.clone(), model.clone(), cx)
            })?;
        }

        Ok(())
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        for codegen in &self.alternatives {
            codegen.update(cx, |codegen, cx| codegen.stop(cx));
        }
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.active_alternative()
            .update(cx, |codegen, cx| codegen.undo(cx));

        self.buffer.update(cx, |buffer, cx| {
            if let Some(transaction_id) = self.initial_transaction_id.take() {
                buffer.undo_transaction(transaction_id, cx);
                buffer.refresh_preview(cx);
            }
        });
    }

    pub fn buffer(&self, cx: &App) -> Entity<MultiBuffer> {
        self.active_alternative().read(cx).buffer.clone()
    }

    pub fn old_buffer(&self, cx: &App) -> Entity<Buffer> {
        self.active_alternative().read(cx).old_buffer.clone()
    }

    pub fn snapshot(&self, cx: &App) -> MultiBufferSnapshot {
        self.active_alternative().read(cx).snapshot.clone()
    }

    pub fn edit_position(&self, cx: &App) -> Option<Anchor> {
        self.active_alternative().read(cx).edit_position
    }

    pub fn diff<'a>(&self, cx: &'a App) -> &'a Diff {
        &self.active_alternative().read(cx).diff
    }

    pub fn last_equal_ranges<'a>(&self, cx: &'a App) -> &'a [Range<Anchor>] {
        self.active_alternative().read(cx).last_equal_ranges()
    }

    pub fn selected_text<'a>(&self, cx: &'a App) -> Option<&'a str> {
        self.active_alternative().read(cx).selected_text()
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }
}

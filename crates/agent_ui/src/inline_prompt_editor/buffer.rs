use super::*;

impl PromptEditor<BufferCodegen> {
    pub fn new_buffer(
        id: InlineAssistId,
        editor_margins: Arc<Mutex<EditorMargins>>,
        prompt_history: VecDeque<String>,
        prompt_buffer: Entity<MultiBuffer>,
        codegen: Entity<BufferCodegen>,
        session_id: Uuid,
        fs: Arc<dyn Fs>,
        thread_store: Entity<ThreadStore>,
        project: WeakEntity<Project>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<PromptEditor<BufferCodegen>>,
    ) -> PromptEditor<BufferCodegen> {
        let codegen_subscription = cx.observe(&codegen, Self::handle_codegen_changed);
        let mode = PromptEditorMode::Buffer {
            id,
            codegen,
            editor_margins,
        };

        let prompt_editor = cx.new(|cx| {
            let mut editor = Editor::new(
                EditorMode::AutoHeight {
                    min_lines: 1,
                    max_lines: Some(Self::MAX_LINES as usize),
                },
                prompt_buffer,
                None,
                window,
                cx,
            );
            editor.set_soft_wrap_mode(language::language_settings::SoftWrap::EditorWidth, cx);
            // Since the prompt editors for all inline assistants are linked,
            // always show the cursor (even when it isn't focused) because
            // typing in one will make what you typed appear in all of them.
            editor.set_show_cursor_when_unfocused(true, cx);
            editor.set_placeholder_text(&Self::placeholder_text(&mode, window, cx), window, cx);
            editor.set_context_menu_options(ContextMenuOptions {
                min_entries_visible: 12,
                max_entries_visible: 12,
                placement: None,
            });

            editor
        });

        let mention_set = cx.new(|_cx| MentionSet::new(project, Some(thread_store.clone())));

        let model_selector_menu_handle = PopoverMenuHandle::default();

        let mut this: PromptEditor<BufferCodegen> = PromptEditor {
            editor: prompt_editor.clone(),
            mention_set,
            workspace,
            model_selector: cx.new(|cx| {
                AgentModelSelector::new(
                    fs,
                    model_selector_menu_handle,
                    prompt_editor.focus_handle(cx),
                    ModelUsageContext::InlineAssistant,
                    window,
                    cx,
                )
            }),
            edited_since_done: false,
            prompt_history,
            prompt_history_ix: None,
            pending_prompt: String::new(),
            _codegen_subscription: codegen_subscription,
            editor_subscriptions: Vec::new(),
            show_rate_limit_notice: false,
            mode,
            session_state: SessionState {
                session_id,
                completion: CompletionState::Pending,
            },
            _phantom: Default::default(),
        };

        this.assign_completion_provider(cx);
        this.subscribe_to_editor(window, cx);
        this
    }

    fn handle_codegen_changed(
        &mut self,
        codegen: Entity<BufferCodegen>,
        cx: &mut Context<PromptEditor<BufferCodegen>>,
    ) {
        match self.codegen_status(cx) {
            CodegenStatus::Idle => {
                self.editor
                    .update(cx, |editor, _| editor.set_read_only(false));
            }
            CodegenStatus::Pending => {
                self.session_state.completion = CompletionState::Pending;
                self.editor
                    .update(cx, |editor, _| editor.set_read_only(true));
            }
            CodegenStatus::Done => {
                let completion = codegen.read(cx).active_completion(cx);
                self.session_state.completion = CompletionState::Generated {
                    completion_text: completion,
                };
                self.edited_since_done = false;
                self.editor
                    .update(cx, |editor, _| editor.set_read_only(false));
            }
            CodegenStatus::Error(_error) => {
                self.edited_since_done = false;
                self.editor
                    .update(cx, |editor, _| editor.set_read_only(false));
            }
        }
    }

    pub fn id(&self) -> InlineAssistId {
        match &self.mode {
            PromptEditorMode::Buffer { id, .. } => *id,
            PromptEditorMode::Terminal { .. } => unreachable!(),
        }
    }

    pub fn codegen(&self) -> &Entity<BufferCodegen> {
        match &self.mode {
            PromptEditorMode::Buffer { codegen, .. } => codegen,
            PromptEditorMode::Terminal { .. } => unreachable!(),
        }
    }

    pub fn mention_set(&self) -> &Entity<MentionSet> {
        &self.mention_set
    }

    pub fn editor_margins(&self) -> &Arc<Mutex<EditorMargins>> {
        match &self.mode {
            PromptEditorMode::Buffer { editor_margins, .. } => editor_margins,
            PromptEditorMode::Terminal { .. } => unreachable!(),
        }
    }
}

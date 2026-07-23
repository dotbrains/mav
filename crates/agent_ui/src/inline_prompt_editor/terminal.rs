use super::*;

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct TerminalInlineAssistId(pub usize);

impl TerminalInlineAssistId {
    pub fn post_inc(&mut self) -> TerminalInlineAssistId {
        let id = *self;
        self.0 += 1;
        id
    }
}

impl PromptEditor<TerminalCodegen> {
    pub fn new_terminal(
        id: TerminalInlineAssistId,
        prompt_history: VecDeque<String>,
        prompt_buffer: Entity<MultiBuffer>,
        codegen: Entity<TerminalCodegen>,
        session_id: Uuid,
        fs: Arc<dyn Fs>,
        thread_store: Entity<ThreadStore>,
        project: WeakEntity<Project>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let codegen_subscription = cx.observe(&codegen, Self::handle_codegen_changed);
        let mode = PromptEditorMode::Terminal {
            id,
            codegen,
            height_in_lines: 1,
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

        let mut this = Self {
            editor: prompt_editor.clone(),
            mention_set,
            workspace,
            model_selector: cx.new(|cx| {
                AgentModelSelector::new(
                    fs,
                    model_selector_menu_handle.clone(),
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
            mode,
            show_rate_limit_notice: false,
            session_state: SessionState {
                session_id,
                completion: CompletionState::Pending,
            },
            _phantom: Default::default(),
        };
        this.count_lines(cx);
        this.assign_completion_provider(cx);
        this.subscribe_to_editor(window, cx);
        this
    }

    fn count_lines(&mut self, cx: &mut Context<Self>) {
        let height_in_lines = cmp::max(
            2, // Make the editor at least two lines tall, to account for padding and buttons.
            cmp::min(
                self.editor
                    .update(cx, |editor, cx| editor.max_point(cx).row().0 + 1),
                Self::MAX_LINES as u32,
            ),
        ) as u8;

        match &mut self.mode {
            PromptEditorMode::Terminal {
                height_in_lines: current_height,
                ..
            } => {
                if height_in_lines != *current_height {
                    *current_height = height_in_lines;
                    cx.emit(PromptEditorEvent::Resized { height_in_lines });
                }
            }
            PromptEditorMode::Buffer { .. } => unreachable!(),
        }
    }

    fn handle_codegen_changed(&mut self, codegen: Entity<TerminalCodegen>, cx: &mut Context<Self>) {
        match &self.codegen().read(cx).status {
            CodegenStatus::Idle => {
                self.editor
                    .update(cx, |editor, _| editor.set_read_only(false));
            }
            CodegenStatus::Pending => {
                self.session_state.completion = CompletionState::Pending;
                self.editor
                    .update(cx, |editor, _| editor.set_read_only(true));
            }
            CodegenStatus::Done | CodegenStatus::Error(_) => {
                self.session_state.completion = CompletionState::Generated {
                    completion_text: codegen.read(cx).completion(),
                };
                self.edited_since_done = false;
                self.editor
                    .update(cx, |editor, _| editor.set_read_only(false));
            }
        }
    }

    pub fn mention_set(&self) -> &Entity<MentionSet> {
        &self.mention_set
    }

    pub fn codegen(&self) -> &Entity<TerminalCodegen> {
        match &self.mode {
            PromptEditorMode::Buffer { .. } => unreachable!(),
            PromptEditorMode::Terminal { codegen, .. } => codegen,
        }
    }

    pub fn id(&self) -> TerminalInlineAssistId {
        match &self.mode {
            PromptEditorMode::Buffer { .. } => unreachable!(),
            PromptEditorMode::Terminal { id, .. } => *id,
        }
    }
}

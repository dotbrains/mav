use super::*;

impl AcpThread {
    pub fn new(
        parent_session_id: Option<acp::SessionId>,
        title: Option<SharedString>,
        work_dirs: Option<PathList>,
        connection: Rc<dyn AgentConnection>,
        project: Entity<Project>,
        action_log: Entity<ActionLog>,
        session_id: acp::SessionId,
        mut prompt_capabilities_rx: watch::Receiver<acp::PromptCapabilities>,
        cx: &mut Context<Self>,
    ) -> Self {
        let prompt_capabilities = prompt_capabilities_rx.borrow().clone();
        let task = cx.spawn::<_, anyhow::Result<()>>(async move |this, cx| {
            loop {
                let caps = prompt_capabilities_rx.recv().await?;
                this.update(cx, |this, cx| {
                    this.prompt_capabilities = caps;
                    cx.emit(AcpThreadEvent::PromptCapabilitiesUpdated);
                })?;
            }
        });

        let git_store = project.read(cx).git_store().clone();
        let _git_store_subscription = cx.subscribe(&git_store, |this, _, event, cx| {
            if matches!(
                event,
                GitStoreEvent::RepositoryUpdated(
                    _,
                    RepositoryEvent::StatusesChanged | RepositoryEvent::HeadChanged,
                    _
                )
            ) {
                this.update_last_checkpoint_if_changed_task =
                    Some(this.update_last_checkpoint_if_changed(cx));
            }
        });

        Self {
            parent_session_id,
            work_dirs,
            action_log,
            _git_store_subscription,
            update_last_checkpoint_if_changed_task: None,
            shared_buffers: Default::default(),
            entries: Default::default(),
            plan: Default::default(),
            title,
            provisional_title: None,
            project,
            running_turn: None,
            turn_id: 0,
            connection,
            session_id,
            token_usage: None,
            cost: None,
            prompt_capabilities,
            available_commands: Vec::new(),
            _observe_prompt_capabilities: task,
            terminals: HashMap::default(),
            pending_terminal_output: HashMap::default(),
            pending_terminal_exit: HashMap::default(),
            had_error: false,
            draft_prompt: None,
            ui_scroll_position: None,
            streaming_text_buffer: None,
        }
    }

    pub fn parent_session_id(&self) -> Option<&acp::SessionId> {
        self.parent_session_id.as_ref()
    }

    pub fn prompt_capabilities(&self) -> acp::PromptCapabilities {
        self.prompt_capabilities.clone()
    }

    pub fn available_commands(&self) -> &[acp::AvailableCommand] {
        &self.available_commands
    }

    pub fn is_draft_thread(&self) -> bool {
        self.entries().is_empty()
    }

    pub fn draft_prompt(&self) -> Option<&[acp::ContentBlock]> {
        self.draft_prompt.as_deref()
    }

    pub fn set_draft_prompt(
        &mut self,
        prompt: Option<Vec<acp::ContentBlock>>,
        cx: &mut Context<Self>,
    ) {
        cx.emit(AcpThreadEvent::PromptUpdated);
        self.draft_prompt = prompt;
    }

    pub fn ui_scroll_position(&self) -> Option<gpui::ListOffset> {
        self.ui_scroll_position
    }

    pub fn set_ui_scroll_position(&mut self, position: Option<gpui::ListOffset>) {
        self.ui_scroll_position = position;
    }

    pub fn connection(&self) -> &Rc<dyn AgentConnection> {
        &self.connection
    }

    pub fn action_log(&self) -> &Entity<ActionLog> {
        &self.action_log
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    pub fn title(&self) -> Option<SharedString> {
        self.title
            .clone()
            .or_else(|| self.provisional_title.clone())
    }

    pub fn has_provisional_title(&self) -> bool {
        self.provisional_title.is_some()
    }

    pub fn entries(&self) -> &[AgentThreadEntry] {
        &self.entries
    }

    pub fn is_compacting(&self) -> bool {
        self.entries.last().is_some_and(|entry| {
            matches!(
                entry,
                AgentThreadEntry::ContextCompaction(compaction) if compaction.is_in_progress()
            )
        })
    }

    pub fn invalidate_mermaid_caches(&self, cx: &mut App) {
        for entry in &self.entries {
            let chunks = match entry {
                AgentThreadEntry::AssistantMessage(message) => &message.chunks,
                _ => continue,
            };
            for chunk in chunks {
                let block = match chunk {
                    AssistantMessageChunk::Message { block, .. } => block,
                    AssistantMessageChunk::Thought { block, .. } => block,
                };
                if let Some(markdown) = block.markdown() {
                    markdown.update(cx, |markdown, cx| {
                        markdown.invalidate_mermaid_cache(cx);
                    });
                }
            }
        }
    }

    pub fn session_id(&self) -> &acp::SessionId {
        &self.session_id
    }

    pub fn supports_truncate(&self, cx: &App) -> bool {
        self.connection.truncate(&self.session_id, cx).is_some()
    }

    pub fn work_dirs(&self) -> Option<&PathList> {
        self.work_dirs.as_ref()
    }

    pub fn set_work_dirs(&mut self, work_dirs: PathList, cx: &mut Context<Self>) {
        self.work_dirs = Some(work_dirs);
        cx.emit(AcpThreadEvent::WorkingDirectoriesUpdated)
    }

    pub fn status(&self) -> ThreadStatus {
        if self.running_turn.is_some() {
            ThreadStatus::Generating
        } else {
            ThreadStatus::Idle
        }
    }

    pub fn had_error(&self) -> bool {
        self.had_error
    }

    pub fn is_waiting_for_confirmation(&self) -> bool {
        for entry in self.entries.iter().rev() {
            match entry {
                AgentThreadEntry::UserMessage(_) => return false,
                AgentThreadEntry::ToolCall(ToolCall {
                    status: ToolCallStatus::WaitingForConfirmation { .. },
                    ..
                }) => return true,
                AgentThreadEntry::ToolCall(_)
                | AgentThreadEntry::AssistantMessage(_)
                | AgentThreadEntry::CompletedPlan(_)
                | AgentThreadEntry::ContextCompaction(_) => {}
            }
        }
        false
    }

    pub fn token_usage(&self) -> Option<&TokenUsage> {
        self.token_usage.as_ref()
    }

    pub fn cost(&self) -> Option<&SessionCost> {
        self.cost.as_ref()
    }

    pub fn has_pending_edit_tool_calls(&self) -> bool {
        for entry in self.entries.iter().rev() {
            match entry {
                AgentThreadEntry::UserMessage(_) => return false,
                AgentThreadEntry::ToolCall(
                    call @ ToolCall {
                        status: ToolCallStatus::InProgress | ToolCallStatus::Pending,
                        ..
                    },
                ) if call.diffs().next().is_some() => {
                    return true;
                }
                AgentThreadEntry::ToolCall(_)
                | AgentThreadEntry::AssistantMessage(_)
                | AgentThreadEntry::CompletedPlan(_)
                | AgentThreadEntry::ContextCompaction(_) => {}
            }
        }

        false
    }

    pub fn has_in_progress_tool_calls(&self) -> bool {
        for entry in self.entries.iter().rev() {
            match entry {
                AgentThreadEntry::UserMessage(_) => return false,
                AgentThreadEntry::ToolCall(ToolCall {
                    status: ToolCallStatus::InProgress | ToolCallStatus::Pending,
                    ..
                }) => {
                    return true;
                }
                AgentThreadEntry::ToolCall(_)
                | AgentThreadEntry::AssistantMessage(_)
                | AgentThreadEntry::CompletedPlan(_)
                | AgentThreadEntry::ContextCompaction(_) => {}
            }
        }

        false
    }

    pub fn used_tools_since_last_user_message(&self) -> bool {
        for entry in self.entries.iter().rev() {
            match entry {
                AgentThreadEntry::UserMessage(..) => return false,
                AgentThreadEntry::AssistantMessage(..)
                | AgentThreadEntry::CompletedPlan(..)
                | AgentThreadEntry::ContextCompaction(_) => continue,
                AgentThreadEntry::ToolCall(..) => return true,
            }
        }

        false
    }
}

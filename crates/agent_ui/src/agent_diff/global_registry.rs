use super::*;

#[derive(Default)]
pub struct AgentDiff {
    reviewing_editors: HashMap<WeakEntity<Editor>, EditorState>,
    workspace_threads: HashMap<WeakEntity<Workspace>, WorkspaceThread>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorState {
    Idle,
    Reviewing,
}

struct WorkspaceThread {
    thread: WeakEntity<AcpThread>,
    _thread_subscriptions: (Subscription, Subscription),
    singleton_editors: HashMap<WeakEntity<Buffer>, HashMap<WeakEntity<Editor>, Subscription>>,
    _settings_subscription: Subscription,
    _workspace_subscription: Option<Subscription>,
}

struct AgentDiffGlobal(Entity<AgentDiff>);

impl Global for AgentDiffGlobal {}

impl AgentDiff {
    fn global(cx: &mut App) -> Entity<Self> {
        cx.try_global::<AgentDiffGlobal>()
            .map(|global| global.0.clone())
            .unwrap_or_else(|| {
                let entity = cx.new(|_cx| Self::default());
                let global = AgentDiffGlobal(entity.clone());
                cx.set_global(global);
                entity
            })
    }

    pub fn set_active_thread(
        workspace: &WeakEntity<Workspace>,
        thread: Entity<AcpThread>,
        window: &mut Window,
        cx: &mut App,
    ) {
        Self::global(cx).update(cx, |this, cx| {
            this.register_active_thread_impl(workspace, thread, window, cx);
        });
    }

    fn register_active_thread_impl(
        &mut self,
        workspace: &WeakEntity<Workspace>,
        thread: Entity<AcpThread>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action_log = thread.read(cx).action_log().clone();

        let action_log_subscription = cx.observe_in(&action_log, window, {
            let workspace = workspace.clone();
            move |this, _action_log, window, cx| {
                this.update_reviewing_editors(&workspace, window, cx);
            }
        });

        let thread_subscription = cx.subscribe_in(&thread, window, {
            let workspace = workspace.clone();
            move |this, thread, event, window, cx| {
                this.handle_acp_thread_event(&workspace, thread, event, window, cx)
            }
        });

        if let Some(workspace_thread) = self.workspace_threads.get_mut(workspace) {
            // replace thread and action log subscription, but keep editors
            workspace_thread.thread = thread.downgrade();
            workspace_thread._thread_subscriptions = (action_log_subscription, thread_subscription);
            self.update_reviewing_editors(workspace, window, cx);
            return;
        }

        let settings_subscription = cx.observe_global_in::<SettingsStore>(window, {
            let workspace = workspace.clone();
            let mut was_active = AgentSettings::get_global(cx).single_file_review;
            move |this, window, cx| {
                let is_active = AgentSettings::get_global(cx).single_file_review;
                if was_active != is_active {
                    was_active = is_active;
                    this.update_reviewing_editors(&workspace, window, cx);
                }
            }
        });

        let workspace_subscription = workspace
            .upgrade()
            .map(|workspace| cx.subscribe_in(&workspace, window, Self::handle_workspace_event));

        self.workspace_threads.insert(
            workspace.clone(),
            WorkspaceThread {
                thread: thread.downgrade(),
                _thread_subscriptions: (action_log_subscription, thread_subscription),
                singleton_editors: HashMap::default(),
                _settings_subscription: settings_subscription,
                _workspace_subscription: workspace_subscription,
            },
        );

        let workspace = workspace.clone();
        cx.defer_in(window, move |this, window, cx| {
            if let Some(workspace) = workspace.upgrade() {
                this.register_workspace(workspace, window, cx);
            }
        });
    }

    fn register_workspace(
        &mut self,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let agent_diff = cx.entity();

        let editors = workspace.update(cx, |workspace, cx| {
            let agent_diff = agent_diff.clone();

            Self::register_review_action::<Keep>(workspace, Self::keep, &agent_diff);
            Self::register_review_action::<Reject>(workspace, Self::reject, &agent_diff);
            Self::register_review_action::<KeepAll>(workspace, Self::keep_all, &agent_diff);
            Self::register_review_action::<RejectAll>(workspace, Self::reject_all, &agent_diff);

            workspace.items_of_type(cx).collect::<Vec<_>>()
        });

        let weak_workspace = workspace.downgrade();

        for editor in editors {
            if let Some(buffer) = Self::full_editor_buffer(editor.read(cx), cx) {
                self.register_editor(weak_workspace.clone(), buffer, editor, window, cx);
            };
        }

        self.update_reviewing_editors(&weak_workspace, window, cx);
    }

    fn register_review_action<T: Action>(
        workspace: &mut Workspace,
        review: impl Fn(
            &Entity<Editor>,
            &Entity<AcpThread>,
            &WeakEntity<Workspace>,
            &mut Window,
            &mut App,
        ) -> PostReviewState
        + 'static,
        this: &Entity<AgentDiff>,
    ) {
        let this = this.clone();
        workspace.register_action(move |workspace, _: &T, window, cx| {
            let review = &review;
            let task = this.update(cx, |this, cx| {
                this.review_in_active_editor(workspace, review, window, cx)
            });

            if let Some(task) = task {
                task.detach_and_log_err(cx);
            } else {
                cx.propagate();
            }
        });
    }

    fn handle_acp_thread_event(
        &mut self,
        workspace: &WeakEntity<Workspace>,
        thread: &Entity<AcpThread>,
        event: &AcpThreadEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AcpThreadEvent::NewEntry => {
                if thread
                    .read(cx)
                    .entries()
                    .last()
                    .is_some_and(|entry| entry.diffs().next().is_some())
                {
                    self.update_reviewing_editors(workspace, window, cx);
                }
            }
            AcpThreadEvent::EntryUpdated(ix) => {
                if thread
                    .read(cx)
                    .entries()
                    .get(*ix)
                    .is_some_and(|entry| entry.diffs().next().is_some())
                {
                    self.update_reviewing_editors(workspace, window, cx);
                }
            }
            AcpThreadEvent::Stopped(_) => {
                self.update_reviewing_editors(workspace, window, cx);
            }
            AcpThreadEvent::Error | AcpThreadEvent::LoadError(_) | AcpThreadEvent::Refusal => {
                self.update_reviewing_editors(workspace, window, cx);
            }
            AcpThreadEvent::TitleUpdated
            | AcpThreadEvent::StatusChanged
            | AcpThreadEvent::TokenUsageUpdated
            | AcpThreadEvent::SubagentSpawned(_)
            | AcpThreadEvent::EntriesRemoved(_)
            | AcpThreadEvent::ToolAuthorizationRequested(_)
            | AcpThreadEvent::ToolAuthorizationReceived(_)
            | AcpThreadEvent::PromptCapabilitiesUpdated
            | AcpThreadEvent::AvailableCommandsUpdated(_)
            | AcpThreadEvent::Retry(_)
            | AcpThreadEvent::ModeUpdated(_)
            | AcpThreadEvent::ConfigOptionsUpdated(_)
            | AcpThreadEvent::WorkingDirectoriesUpdated
            | AcpThreadEvent::PromptUpdated => {}
        }
    }

    fn handle_workspace_event(
        &mut self,
        workspace: &Entity<Workspace>,
        event: &workspace::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let workspace::Event::ItemAdded { item } = event
            && let Some(editor) = item.downcast::<Editor>()
            && let Some(buffer) = Self::full_editor_buffer(editor.read(cx), cx)
        {
            self.register_editor(workspace.downgrade(), buffer, editor, window, cx);
        }
    }

    fn full_editor_buffer(editor: &Editor, cx: &App) -> Option<WeakEntity<Buffer>> {
        if editor.mode().is_full() {
            editor
                .buffer()
                .read(cx)
                .as_singleton()
                .map(|buffer| buffer.downgrade())
        } else {
            None
        }
    }
}

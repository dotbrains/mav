use super::*;

impl NativeAgent {
    pub fn new(
        thread_store: Entity<ThreadStore>,
        templates: Arc<Templates>,
        fs: Arc<dyn Fs>,
        cx: &mut App,
    ) -> Entity<NativeAgent> {
        log::debug!("Creating new NativeAgent");

        cx.new(|cx| {
            let subscriptions = vec![
                cx.subscribe(
                    &LanguageModelRegistry::global(cx),
                    Self::handle_models_updated_event,
                ),
                // Flush thread content on quit so an in-flight async save
                // can't leave a thread orphaned ("no thread found with ID").
                cx.on_app_quit(Self::flush_threads_on_quit),
            ];

            if !cx.has_global::<SkillIndex>() {
                cx.set_global(SkillIndex::default());
            }

            Self {
                sessions: HashMap::default(),
                pending_sessions: HashMap::default(),
                thread_store,
                projects: HashMap::default(),
                templates,
                models: LanguageModels::new(cx),
                sibling_thread_host: None,
                fs,
                _subscriptions: subscriptions,
                skills_state: SkillsState::default(),
            }
        })
    }

    pub fn set_sibling_thread_host(&mut self, host: Rc<dyn SiblingThreadHost>) {
        self.sibling_thread_host = Some(host);
    }

    pub fn sibling_thread_host(&self) -> Option<Rc<dyn SiblingThreadHost>> {
        self.sibling_thread_host.clone()
    }

    pub(super) fn new_session(
        &mut self,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Entity<AcpThread> {
        let project_id = self.get_or_create_project_state(&project, cx);
        let project_state = &self.projects[&project_id];

        let registry = LanguageModelRegistry::read_global(cx);
        let available_count = registry.available_models(cx).count();
        log::debug!("Total available models: {}", available_count);

        let default_model = registry.default_model().and_then(|default_model| {
            self.models
                .model_from_id(&LanguageModels::model_id(&default_model.model))
        });
        let thread = cx.new(|cx| {
            Thread::new(
                project,
                project_state.project_context.clone(),
                project_state.context_server_registry.clone(),
                self.templates.clone(),
                default_model,
                cx,
            )
        });

        self.register_session(thread, project_id, 1, cx)
    }

    pub(super) fn register_session(
        &mut self,
        thread_handle: Entity<Thread>,
        project_id: EntityId,
        ref_count: usize,
        cx: &mut Context<Self>,
    ) -> Entity<AcpThread> {
        let connection = Rc::new(NativeAgentConnection(cx.entity()));

        let thread = thread_handle.read(cx);
        let session_id = thread.id().clone();
        let parent_session_id = thread.parent_thread_id();
        let title = thread.title();
        let draft_prompt = thread.draft_prompt().map(Vec::from);
        let scroll_position = thread.ui_scroll_position();
        let token_usage = thread.latest_token_usage();
        let project = thread.project.clone();
        let action_log = thread.action_log.clone();
        let prompt_capabilities_rx = thread.prompt_capabilities_rx.clone();
        let acp_thread = cx.new(|cx| {
            let mut acp_thread = acp_thread::AcpThread::new(
                parent_session_id,
                title,
                None,
                connection,
                project.clone(),
                action_log.clone(),
                session_id.clone(),
                prompt_capabilities_rx,
                cx,
            );
            acp_thread.set_draft_prompt(draft_prompt, cx);
            acp_thread.set_ui_scroll_position(scroll_position);
            acp_thread.update_token_usage(token_usage, cx);
            acp_thread
        });

        let registry = LanguageModelRegistry::read_global(cx);
        let summarization_model = registry.thread_summary_model(cx).map(|c| c.model);

        let weak = cx.weak_entity();
        let weak_thread = thread_handle.downgrade();
        thread_handle.update(cx, |thread, cx| {
            thread.set_summarization_model(summarization_model, cx);
            thread.add_default_tools(
                Rc::new(NativeThreadEnvironment {
                    acp_thread: acp_thread.downgrade(),
                    thread: weak_thread,
                    agent: weak.clone(),
                }) as _,
                cx,
            );
            // The resolver closure reads `state.skills` at invocation
            // time, so skills added or removed by the SKILL.md watcher
            // after the thread is constructed are still visible to the
            // model — without this, the catalog and tool would drift out
            // of sync until the session was reopened.
            thread.add_tool(SkillTool::with_body_resolver(
                skills_resolver_for_project(weak.clone(), project_id),
                skill_body_resolver_for_project(project.clone(), self.fs.clone()),
            ));
        });

        let subscriptions = vec![
            cx.subscribe(&thread_handle, Self::handle_thread_title_updated),
            cx.subscribe(&thread_handle, Self::handle_thread_token_usage_updated),
            cx.observe(&thread_handle, move |this, thread, cx| {
                this.save_thread(thread, cx)
            }),
        ];

        self.sessions.insert(
            session_id,
            Session {
                thread: thread_handle,
                acp_thread: acp_thread.clone(),
                project_id,
                _subscriptions: subscriptions,
                pending_save: Task::ready(Ok(())),
                ref_count,
            },
        );

        self.update_available_commands_for_project(project_id, cx);

        acp_thread
    }

    pub fn models(&self) -> &LanguageModels {
        &self.models
    }
}

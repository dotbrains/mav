use super::*;

impl AgentPanel {
    fn should_render_trial_end_upsell(&self, cx: &mut Context<Self>) -> bool {
        if TrialEndUpsell::dismissed(cx) {
            return false;
        }

        match &self.base_view {
            BaseView::AgentThread { .. } => {
                if LanguageModelRegistry::global(cx)
                    .read(cx)
                    .default_model()
                    .is_some_and(|model| {
                        model.provider.id() != language_model::MAV_CLOUD_PROVIDER_ID
                    })
                {
                    return false;
                }
            }
            BaseView::Terminal { .. } | BaseView::Uninitialized => {
                return false;
            }
        }

        let plan = self.user_store.read(cx).plan();
        let has_previous_trial = self.user_store.read(cx).trial_started_at().is_some();

        plan.is_some_and(|plan| plan == Plan::MavFree) && has_previous_trial
    }

    pub(super) fn dismiss_ai_onboarding(&mut self, cx: &mut Context<Self>) {
        self.new_user_onboarding_upsell_dismissed
            .store(true, Ordering::Release);
        OnboardingUpsell::set_dismissed(true, cx);
        cx.notify();
    }

    fn should_render_new_user_onboarding(&mut self, cx: &mut Context<Self>) -> bool {
        if self
            .new_user_onboarding_upsell_dismissed
            .load(Ordering::Acquire)
        {
            return false;
        }

        let user_store = self.user_store.read(cx);

        if user_store.plan().is_some_and(|plan| plan == Plan::MavPro)
            && user_store
                .subscription_period()
                .and_then(|period| period.0.checked_add_days(chrono::Days::new(1)))
                .is_some_and(|date| date < chrono::Utc::now())
        {
            if !self
                .new_user_onboarding_upsell_dismissed
                .load(Ordering::Acquire)
            {
                self.dismiss_ai_onboarding(cx);
            }
            return false;
        }

        let has_configured_non_mav_providers = LanguageModelRegistry::read_global(cx)
            .visible_providers()
            .iter()
            .any(|provider| {
                provider.is_authenticated(cx)
                    && provider.id() != language_model::MAV_CLOUD_PROVIDER_ID
            });

        match &self.base_view {
            BaseView::Uninitialized | BaseView::Terminal { .. } => false,
            BaseView::AgentThread { conversation_view } => {
                if conversation_view.read(cx).as_native_thread(cx).is_some() {
                    let history_is_empty = ThreadStore::global(cx).read(cx).is_empty();
                    history_is_empty || !has_configured_non_mav_providers
                } else {
                    false
                }
            }
        }
    }

    pub(super) fn render_new_user_onboarding(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !self.should_render_new_user_onboarding(cx) {
            return None;
        }

        Some(
            div()
                .bg(cx.theme().colors().editor_background)
                .child(self.new_user_onboarding.clone()),
        )
    }

    pub(super) fn render_trial_end_upsell(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !self.should_render_trial_end_upsell(cx) {
            return None;
        }

        Some(
            v_flex()
                .absolute()
                .inset_0()
                .size_full()
                .bg(cx.theme().colors().panel_background)
                .opacity(0.85)
                .block_mouse_except_scroll()
                .child(EndTrialUpsell::new(Arc::new({
                    let this = cx.entity();
                    move |_, cx| {
                        this.update(cx, |_this, cx| {
                            TrialEndUpsell::set_dismissed(true, cx);
                            cx.notify();
                        });
                    }
                }))),
        )
    }

    pub(super) fn render_drag_target(&self, cx: &Context<Self>) -> Div {
        let is_local = self.project.read(cx).is_local();
        div()
            .invisible()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .bg(cx.theme().colors().drop_target_background)
            .drag_over::<DraggedTab>(|this, _, _, _| this.visible())
            .drag_over::<DraggedSelection>(|this, _, _, _| this.visible())
            .when(is_local, |this| {
                this.drag_over::<ExternalPaths>(|this, _, _, _| this.visible())
            })
            .on_drop(cx.listener(move |this, tab: &DraggedTab, window, cx| {
                let item = tab.pane.read(cx).item_for_index(tab.ix);
                let project_paths = item
                    .and_then(|item| item.project_path(cx))
                    .into_iter()
                    .collect::<Vec<_>>();
                this.handle_drop(project_paths, vec![], window, cx);
            }))
            .on_drop(
                cx.listener(move |this, selection: &DraggedSelection, window, cx| {
                    let project_paths = selection
                        .items()
                        .filter_map(|item| this.project.read(cx).path_for_entry(item.entry_id, cx))
                        .collect::<Vec<_>>();
                    this.handle_drop(project_paths, vec![], window, cx);
                }),
            )
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| {
                this.handle_external_paths_drop(paths, window, cx);
            }))
    }

    pub(super) fn handle_external_paths_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(&self.base_view, BaseView::Terminal { .. }) {
            // Terminal drops should match normal terminal views by pasting raw OS paths.
            // The agent-thread path below converts paths to project paths, which can add
            // worktrees and is only needed when attaching files to a conversation.
            self.paste_external_paths_into_active_terminal(paths, window, cx);
            return;
        }

        let BaseView::AgentThread { conversation_view } = &self.base_view else {
            return;
        };
        let conversation_view = conversation_view.clone();
        let tasks = paths
            .paths()
            .iter()
            .map(|path| Workspace::project_path_for_path(self.project.clone(), path, false, cx))
            .collect::<Vec<_>>();
        cx.spawn_in(window, async move |_this, cx| {
            let mut paths = vec![];
            let mut added_worktrees = vec![];
            let opened_paths = futures::future::join_all(tasks).await;
            for entry in opened_paths {
                if let Some((worktree, project_path)) = entry.log_err() {
                    added_worktrees.push(worktree);
                    paths.push(project_path);
                }
            }
            conversation_view
                .update_in(cx, |conversation_view, window, cx| {
                    conversation_view.insert_dragged_files(paths, added_worktrees, window, cx);
                })
                .log_err();
        })
        .detach();
    }

    pub(super) fn paste_external_paths_into_active_terminal(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let BaseView::Terminal { terminal_id } = &self.base_view else {
            return;
        };

        if !self.project.read(cx).is_local() {
            return;
        }

        let Some(terminal_view) = self
            .terminals
            .get(terminal_id)
            .map(|terminal| terminal.view.clone())
        else {
            return;
        };

        terminal_view.update(cx, |terminal_view, cx| {
            terminal_view.add_paths_to_terminal(paths.paths(), window, cx);
        });
    }

    fn handle_drop(
        &mut self,
        paths: Vec<ProjectPath>,
        added_worktrees: Vec<Entity<Worktree>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.base_view {
            BaseView::AgentThread { conversation_view } => {
                conversation_view.update(cx, |conversation_view, cx| {
                    conversation_view.insert_dragged_files(paths, added_worktrees, window, cx);
                });
            }
            BaseView::Terminal { terminal_id } => {
                let paths = {
                    let project = self.project.read(cx);
                    paths
                        .iter()
                        .filter_map(|project_path| project.absolute_path(project_path, cx))
                        .collect::<Vec<_>>()
                };

                if paths.is_empty() {
                    return;
                }

                if let Some(terminal_view) = self
                    .terminals
                    .get(terminal_id)
                    .map(|terminal| terminal.view.clone())
                {
                    terminal_view.update(cx, |terminal_view, cx| {
                        terminal_view.add_paths_to_terminal(&paths, window, cx);
                    });
                }
            }
            BaseView::Uninitialized => {}
        }
    }

    pub(super) fn key_context(&self) -> KeyContext {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("AgentPanel");
        key_context
    }
}

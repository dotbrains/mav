use super::*;

impl ConversationView {
    pub(super) fn notify_with_sound(
        &mut self,
        caption: impl Into<SharedString>,
        icon: IconName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "audio")]
        self.play_notification_sound(window, cx);
        self.show_notification(caption, icon, window, cx);
    }

    fn is_visible(&self, multi_workspace: &Entity<MultiWorkspace>, cx: &Context<Self>) -> bool {
        let Some(workspace) = self.workspace.upgrade() else {
            return false;
        };

        let multi_workspace = multi_workspace.read(cx);
        multi_workspace.sidebar_open() && multi_workspace.is_threads_list_view_active(cx)
            || multi_workspace.workspace() == &workspace
                && self.is_visible_in_agent_panel(&workspace, cx)
    }

    fn is_visible_in_agent_panel(&self, workspace: &Entity<Workspace>, cx: &Context<Self>) -> bool {
        AgentPanel::is_visible(workspace, cx)
            && workspace
                .read(cx)
                .panel::<AgentPanel>(cx)
                .is_some_and(|panel| {
                    panel
                        .read(cx)
                        .visible_conversation_view()
                        .map(|conversation_view| conversation_view.entity_id())
                        == Some(cx.entity_id())
                })
    }

    fn agent_status_visible(&self, window: &Window, cx: &Context<Self>) -> bool {
        if !window.is_window_active() {
            return false;
        }

        if let Some(multi_workspace) = window.root::<MultiWorkspace>().flatten() {
            self.is_visible(&multi_workspace, cx)
        } else {
            self.workspace
                .upgrade()
                .is_some_and(|workspace| self.is_visible_in_agent_panel(&workspace, cx))
        }
    }

    #[cfg(feature = "audio")]
    fn play_notification_sound(&self, window: &Window, cx: &mut Context<Self>) {
        let visible = window.is_window_active()
            && if let Some(mw) = window.root::<MultiWorkspace>().flatten() {
                self.is_visible(&mw, cx)
            } else {
                self.workspace
                    .upgrade()
                    .is_some_and(|workspace| self.is_visible_in_agent_panel(&workspace, cx))
            };
        let settings = AgentSettings::get_global(cx);
        if settings.play_sound_when_agent_done.should_play(visible) {
            Audio::play_sound(Sound::AgentDone, cx);
        }
    }

    fn show_notification(
        &mut self,
        caption: impl Into<SharedString>,
        icon: IconName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.notifications.is_empty() {
            return;
        }

        let settings = AgentSettings::get_global(cx);

        let should_notify = !self.agent_status_visible(window, cx);

        if !should_notify {
            return;
        }

        let Some(root_thread) = self.root_thread_view() else {
            return;
        };
        let root_thread = root_thread.read(cx).thread.read(cx);
        let root_thread_id = self.thread_id;
        let root_work_dirs = root_thread.work_dirs().cloned();
        let root_title = root_thread.title();

        let title = root_title
            .clone()
            .unwrap_or_else(|| self.agent.agent_id().0);

        match settings.notify_when_agent_waiting {
            NotifyWhenAgentWaiting::PrimaryScreen => {
                if let Some(primary) = cx.primary_display() {
                    self.pop_up(
                        icon,
                        caption.into(),
                        title,
                        root_thread_id,
                        root_work_dirs,
                        root_title,
                        window,
                        primary,
                        cx,
                    );
                }
            }
            NotifyWhenAgentWaiting::AllScreens => {
                let caption = caption.into();
                for screen in cx.displays() {
                    self.pop_up(
                        icon,
                        caption.clone(),
                        title.clone(),
                        root_thread_id,
                        root_work_dirs.clone(),
                        root_title.clone(),
                        window,
                        screen,
                        cx,
                    );
                }
            }
            NotifyWhenAgentWaiting::Never => {}
        }
    }

    fn pop_up(
        &mut self,
        icon: IconName,
        caption: SharedString,
        title: SharedString,
        root_thread_id: ThreadId,
        root_work_dirs: Option<PathList>,
        root_title: Option<SharedString>,
        window: &mut Window,
        screen: Rc<dyn PlatformDisplay>,
        cx: &mut Context<Self>,
    ) {
        let options = AgentNotification::window_options(screen, cx);

        let project_name = self.workspace.upgrade().and_then(|workspace| {
            workspace
                .read(cx)
                .project()
                .read(cx)
                .visible_worktrees(cx)
                .next()
                .map(|worktree| worktree.read(cx).root_name_str().to_string())
        });

        if let Some(screen_window) = cx
            .open_window(options, |_window, cx| {
                cx.new(|_cx| {
                    AgentNotification::new(title.clone(), Some(caption.clone()), icon, project_name)
                })
            })
            .log_err()
            && let Some(pop_up) = screen_window.entity(cx).log_err()
        {
            self.notification_subscriptions
                .entry(screen_window)
                .or_insert_with(Vec::new)
                .push(cx.subscribe_in(&pop_up, window, {
                    move |this, _, event, window, cx| match event {
                        AgentNotificationEvent::Accepted => {
                            let Some(handle) = window.window_handle().downcast::<MultiWorkspace>()
                            else {
                                log::error!("root view should be a MultiWorkspace");
                                return;
                            };
                            cx.activate(true);

                            let workspace_handle = this.workspace.clone();
                            let agent = this.connection_key.clone();
                            let root_work_dirs = root_work_dirs.clone();
                            let root_title = root_title.clone();

                            cx.defer(move |cx| {
                                handle
                                    .update(cx, |multi_workspace, window, cx| {
                                        window.activate_window();
                                        if let Some(workspace) = workspace_handle.upgrade() {
                                            multi_workspace.activate(
                                                workspace.clone(),
                                                None,
                                                window,
                                                cx,
                                            );
                                            workspace.update(cx, |workspace, cx| {
                                                workspace.reveal_panel::<AgentPanel>(window, cx);
                                                if let Some(panel) =
                                                    workspace.panel::<AgentPanel>(cx)
                                                {
                                                    panel.update(cx, |panel, cx| {
                                                        panel.load_agent_thread(
                                                            agent.clone(),
                                                            root_thread_id,
                                                            root_work_dirs.clone(),
                                                            root_title.clone(),
                                                            true,
                                                            AgentThreadSource::AgentPanel,
                                                            window,
                                                            cx,
                                                        );
                                                    });
                                                }
                                                workspace.focus_panel::<AgentPanel>(window, cx);
                                            });
                                        }
                                    })
                                    .log_err();
                            });

                            this.dismiss_notifications(cx);
                        }
                        AgentNotificationEvent::Dismissed => {
                            this.dismiss_notifications(cx);
                        }
                    }
                }));

            self.notifications.push(screen_window);

            let dismiss_if_visible = {
                let pop_up_weak = pop_up.downgrade();
                move |this: &ConversationView,
                      window: &mut Window,
                      cx: &mut Context<ConversationView>| {
                    if this.agent_status_visible(window, cx)
                        && let Some(pop_up) = pop_up_weak.upgrade()
                    {
                        pop_up.update(cx, |notification, cx| {
                            notification.dismiss(cx);
                        });
                    }
                }
            };

            let subscriptions = self
                .notification_subscriptions
                .entry(screen_window)
                .or_insert_with(Vec::new);

            subscriptions.push({
                let dismiss_if_visible = dismiss_if_visible.clone();
                cx.observe_window_activation(window, move |this, window, cx| {
                    dismiss_if_visible(this, window, cx);
                })
            });

            if let Some(multi_workspace) = window.root::<MultiWorkspace>().flatten() {
                let dismiss_if_visible = dismiss_if_visible.clone();
                subscriptions.push(cx.observe_in(
                    &multi_workspace,
                    window,
                    move |this, _, window, cx| {
                        dismiss_if_visible(this, window, cx);
                    },
                ));
            }

            if let Some(panel) = self
                .workspace
                .upgrade()
                .and_then(|workspace| workspace.read(cx).panel::<AgentPanel>(cx))
            {
                subscriptions.push(cx.subscribe_in(
                    &panel,
                    window,
                    move |this, _, event: &AgentPanelEvent, window, cx| match event {
                        AgentPanelEvent::ActiveViewChanged | AgentPanelEvent::ActiveViewFocused => {
                            dismiss_if_visible(this, window, cx);
                        }
                        AgentPanelEvent::EntryChanged
                        | AgentPanelEvent::TerminalClosed { .. }
                        | AgentPanelEvent::ThreadInteracted { .. } => {}
                    },
                ));
            }
        }
    }

    pub(crate) fn dismiss_notifications(&mut self, cx: &mut Context<Self>) -> bool {
        let had_notifications = !self.notifications.is_empty();
        for window in self.notifications.drain(..) {
            window
                .update(cx, |_, window, _| {
                    window.remove_window();
                })
                .ok();

            self.notification_subscriptions.remove(&window);
        }
        had_notifications
    }
}

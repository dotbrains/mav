use super::*;

impl AgentPanel {
    pub(super) fn set_base_view(
        &mut self,
        new_view: BaseView,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_overlay_state();

        let old_view = std::mem::replace(&mut self.base_view, new_view);
        self.retain_running_thread(old_view, cx);

        if let BaseView::AgentThread { conversation_view } = &self.base_view {
            let conversation_view = conversation_view.read(cx);
            let thread_agent = conversation_view.agent_key().clone();
            if self.selected_agent != thread_agent {
                self.selected_agent = thread_agent;
                self.serialize(cx);
            }
        }

        self.refresh_base_view_subscriptions(window, cx);

        if focus {
            self.focus_handle(cx).focus(window, cx);
        }
        cx.emit(AgentPanelEvent::ActiveViewChanged);
    }

    pub(super) fn set_overlay(
        &mut self,
        overlay: OverlayView,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.overlay_view = Some(overlay);
        if focus {
            self.focus_handle(cx).focus(window, cx);
        }
        cx.emit(AgentPanelEvent::ActiveViewChanged);
    }

    pub(super) fn clear_overlay(
        &mut self,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_overlay_state();

        if focus {
            self.focus_handle(cx).focus(window, cx);
        }
        cx.emit(AgentPanelEvent::ActiveViewChanged);
    }

    pub(super) fn clear_overlay_state(&mut self) {
        self.overlay_view = None;
        self.configuration_subscription = None;
        self.configuration = None;
    }

    pub(super) fn refresh_base_view_subscriptions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._base_view_observation = match &self.base_view {
            BaseView::AgentThread { conversation_view } => {
                self._thread_view_subscription =
                    Self::subscribe_to_active_thread_view(conversation_view, window, cx);
                let focus_handle = conversation_view.focus_handle(cx);
                self._active_thread_focus_subscription =
                    Some(cx.on_focus_in(&focus_handle, window, |_this, _window, cx| {
                        cx.emit(AgentPanelEvent::ActiveViewFocused);
                        cx.notify();
                    }));
                let cv = conversation_view.clone();
                self.observe_active_draft_for_empty_editor(&cv, cx);
                Some(cx.observe_in(&cv, window, |this, server_view, window, cx| {
                    this._thread_view_subscription =
                        Self::subscribe_to_active_thread_view(&server_view, window, cx);
                    this.observe_active_draft_for_empty_editor(&server_view, cx);
                    cx.emit(AgentPanelEvent::ActiveViewChanged);
                    this.serialize(cx);
                    cx.notify();
                }))
            }
            BaseView::Terminal { terminal_id } => {
                self._thread_view_subscription = None;
                if let Some(terminal) = self.terminals.get(terminal_id) {
                    let terminal_id = *terminal_id;
                    let focus_handle = terminal.view.focus_handle(cx);
                    self._active_thread_focus_subscription =
                        Some(
                            cx.on_focus_in(&focus_handle, window, move |this, _window, cx| {
                                if let Some(terminal) = this.terminals.get_mut(&terminal_id) {
                                    terminal.has_notification = false;
                                }
                                cx.emit(AgentPanelEvent::ActiveViewFocused);
                                cx.notify();
                            }),
                        );
                } else {
                    self._active_thread_focus_subscription = None;
                }
                None
            }
            BaseView::Uninitialized => {
                self._thread_view_subscription = None;
                self._active_thread_focus_subscription = None;
                None
            }
        };
        self.serialize(cx);
    }

    pub(super) fn visible_surface(&self) -> VisibleSurface<'_> {
        if let Some(overlay_view) = &self.overlay_view {
            return match overlay_view {
                OverlayView::Configuration => {
                    VisibleSurface::Configuration(self.configuration.as_ref())
                }
            };
        }

        match &self.base_view {
            BaseView::Uninitialized => VisibleSurface::Uninitialized,
            BaseView::AgentThread { conversation_view } => {
                VisibleSurface::AgentThread(conversation_view)
            }
            BaseView::Terminal { terminal_id } => self
                .terminals
                .get(terminal_id)
                .map(|terminal| VisibleSurface::Terminal(&terminal.view))
                .unwrap_or(VisibleSurface::Uninitialized),
        }
    }

    pub(super) fn is_overlay_open(&self) -> bool {
        self.overlay_view.is_some()
    }

    pub(super) fn visible_font_size(&self) -> WhichFontSize {
        self.overlay_view.as_ref().map_or_else(
            || self.base_view.which_font_size_used(),
            OverlayView::which_font_size_used,
        )
    }

    fn subscribe_to_active_thread_view(
        server_view: &Entity<ConversationView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Subscription> {
        server_view.read(cx).root_thread_view().map(|tv| {
            cx.subscribe_in(
                &tv,
                window,
                |this, _view, event: &AcpThreadViewEvent, _window, cx| match event {
                    AcpThreadViewEvent::Interacted => {
                        let Some(thread_id) = this.active_thread_id(cx) else {
                            return;
                        };
                        // If the draft was the active thread, it has now been
                        // promoted to a real thread. Clear the ephemeral
                        // pointer; the ConversationView itself stays put as
                        // the active base view.
                        if this
                            .draft_thread
                            .as_ref()
                            .is_some_and(|draft| draft.read(cx).thread_id == thread_id)
                        {
                            this.draft_thread = None;
                            this._draft_editor_observation = None;
                        }
                        this.retained_threads.remove(&thread_id);
                        cx.emit(AgentPanelEvent::ThreadInteracted { thread_id });
                    }
                },
            )
        })
    }
}

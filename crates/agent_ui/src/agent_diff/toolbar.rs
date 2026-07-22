use super::*;

pub struct AgentDiffToolbar {
    active_item: Option<AgentDiffToolbarItem>,
    _settings_subscription: Subscription,
}

pub enum AgentDiffToolbarItem {
    Pane(WeakEntity<AgentDiffPane>),
    Editor {
        editor: WeakEntity<Editor>,
        state: EditorState,
        _diff_subscription: Subscription,
    },
}

impl AgentDiffToolbar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            active_item: None,
            _settings_subscription: cx.observe_global::<SettingsStore>(Self::update_location),
        }
    }

    fn dispatch_action(&self, action: &dyn Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_item) = self.active_item.as_ref() else {
            return;
        };

        match active_item {
            AgentDiffToolbarItem::Pane(agent_diff) => {
                if let Some(agent_diff) = agent_diff.upgrade() {
                    agent_diff.focus_handle(cx).focus(window, cx);
                }
            }
            AgentDiffToolbarItem::Editor { editor, .. } => {
                if let Some(editor) = editor.upgrade() {
                    editor.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
        }

        let action = action.boxed_clone();
        cx.defer(move |cx| {
            cx.dispatch_action(action.as_ref());
        })
    }

    fn handle_diff_notify(&mut self, agent_diff: Entity<AgentDiff>, cx: &mut Context<Self>) {
        let Some(AgentDiffToolbarItem::Editor { editor, state, .. }) = self.active_item.as_mut()
        else {
            return;
        };

        *state = agent_diff.read(cx).editor_state(editor);
        self.update_location(cx);
        cx.notify();
    }

    fn update_location(&mut self, cx: &mut Context<Self>) {
        let location = self.location(cx);
        cx.emit(ToolbarItemEvent::ChangeLocation(location));
    }

    fn location(&self, cx: &App) -> ToolbarItemLocation {
        if !EditorSettings::get_global(cx).toolbar.agent_review {
            return ToolbarItemLocation::Hidden;
        }

        match &self.active_item {
            None => ToolbarItemLocation::Hidden,
            Some(AgentDiffToolbarItem::Pane(_)) => ToolbarItemLocation::PrimaryRight,
            Some(AgentDiffToolbarItem::Editor { state, .. }) => match state {
                EditorState::Reviewing => ToolbarItemLocation::PrimaryRight,
                EditorState::Idle => ToolbarItemLocation::Hidden,
            },
        }
    }
}

impl EventEmitter<ToolbarItemEvent> for AgentDiffToolbar {}

impl ToolbarItemView for AgentDiffToolbar {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        if let Some(item) = active_pane_item {
            if let Some(pane) = item.act_as::<AgentDiffPane>(cx) {
                self.active_item = Some(AgentDiffToolbarItem::Pane(pane.downgrade()));
                return self.location(cx);
            }

            if let Some(editor) = item.act_as::<Editor>(cx)
                && editor.read(cx).mode().is_full()
            {
                let agent_diff = AgentDiff::global(cx);

                self.active_item = Some(AgentDiffToolbarItem::Editor {
                    editor: editor.downgrade(),
                    state: agent_diff.read(cx).editor_state(&editor.downgrade()),
                    _diff_subscription: cx.observe(&agent_diff, Self::handle_diff_notify),
                });

                return self.location(cx);
            }
        }

        self.active_item = None;
        self.location(cx)
    }

    fn pane_focus_update(
        &mut self,
        _pane_focused: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

impl Render for AgentDiffToolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let spinner_icon = div()
            .px_0p5()
            .id("generating")
            .tooltip(Tooltip::text("Generating Changes…"))
            .child(
                Icon::new(IconName::LoadCircle)
                    .size(IconSize::Small)
                    .color(Color::Accent)
                    .with_rotate_animation(3),
            )
            .into_any();

        let Some(active_item) = self.active_item.as_ref() else {
            return Empty.into_any();
        };

        match active_item {
            AgentDiffToolbarItem::Editor { editor, state, .. } => {
                let Some(editor) = editor.upgrade() else {
                    return Empty.into_any();
                };

                let editor_focus_handle = editor.read(cx).focus_handle(cx);

                let content = match state {
                    EditorState::Idle => return Empty.into_any(),
                    EditorState::Reviewing => vec![
                        h_flex()
                            .child(
                                IconButton::new("hunk-up", IconName::ArrowUp)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::for_action_title_in(
                                        "Previous Hunk",
                                        &GoToPreviousHunk,
                                        &editor_focus_handle,
                                    ))
                                    .on_click({
                                        let editor_focus_handle = editor_focus_handle.clone();
                                        move |_, window, cx| {
                                            editor_focus_handle.dispatch_action(
                                                &GoToPreviousHunk,
                                                window,
                                                cx,
                                            );
                                        }
                                    }),
                            )
                            .child(
                                IconButton::new("hunk-down", IconName::ArrowDown)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::for_action_title_in(
                                        "Next Hunk",
                                        &GoToHunk,
                                        &editor_focus_handle,
                                    ))
                                    .on_click({
                                        let editor_focus_handle = editor_focus_handle.clone();
                                        move |_, window, cx| {
                                            editor_focus_handle
                                                .dispatch_action(&GoToHunk, window, cx);
                                        }
                                    }),
                            )
                            .into_any_element(),
                        vertical_divider().into_any_element(),
                        h_flex()
                            .gap_0p5()
                            .child(
                                Button::new("reject-all", "Reject All")
                                    .key_binding({
                                        KeyBinding::for_action_in(
                                            &RejectAll,
                                            &editor_focus_handle,
                                            cx,
                                        )
                                        .map(|kb| kb.size(rems_from_px(12.)))
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dispatch_action(&RejectAll, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("keep-all", "Keep All")
                                    .key_binding({
                                        KeyBinding::for_action_in(
                                            &KeepAll,
                                            &editor_focus_handle,
                                            cx,
                                        )
                                        .map(|kb| kb.size(rems_from_px(12.)))
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dispatch_action(&KeepAll, window, cx)
                                    })),
                            )
                            .into_any_element(),
                    ],
                };

                h_flex()
                    .track_focus(&editor_focus_handle)
                    .size_full()
                    .px_1()
                    .mr_1()
                    .gap_1()
                    .children(content)
                    .child(vertical_divider())
                    .when_some(editor.read(cx).workspace(), |this, _workspace| {
                        this.child(
                            IconButton::new("review", IconName::ListTodo)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::for_action_title_in(
                                    "Review All Files",
                                    &OpenAgentDiff,
                                    &editor_focus_handle,
                                ))
                                .on_click({
                                    cx.listener(move |this, _, window, cx| {
                                        this.dispatch_action(&OpenAgentDiff, window, cx);
                                    })
                                }),
                        )
                    })
                    .child(vertical_divider())
                    .on_action({
                        let editor = editor.clone();
                        move |_action: &OpenAgentDiff, window, cx| {
                            AgentDiff::global(cx).update(cx, |agent_diff, cx| {
                                agent_diff.deploy_pane_from_editor(&editor, window, cx);
                            });
                        }
                    })
                    .into_any()
            }
            AgentDiffToolbarItem::Pane(agent_diff) => {
                let Some(agent_diff) = agent_diff.upgrade() else {
                    return Empty.into_any();
                };

                let has_pending_edit_tool_use = agent_diff
                    .read(cx)
                    .thread
                    .read(cx)
                    .has_pending_edit_tool_calls();

                if has_pending_edit_tool_use {
                    return div().px_2().child(spinner_icon).into_any();
                }

                let is_empty = agent_diff.read(cx).multibuffer.read(cx).is_empty();
                if is_empty {
                    return Empty.into_any();
                }

                let focus_handle = agent_diff.focus_handle(cx);

                h_group_xl()
                    .my_neg_1()
                    .py_1()
                    .items_center()
                    .flex_wrap()
                    .child(
                        h_group_sm()
                            .child(
                                Button::new("reject-all", "Reject All")
                                    .key_binding({
                                        KeyBinding::for_action_in(&RejectAll, &focus_handle, cx)
                                            .map(|kb| kb.size(rems_from_px(12.)))
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dispatch_action(&RejectAll, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("keep-all", "Keep All")
                                    .key_binding({
                                        KeyBinding::for_action_in(&KeepAll, &focus_handle, cx)
                                            .map(|kb| kb.size(rems_from_px(12.)))
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dispatch_action(&KeepAll, window, cx)
                                    })),
                            ),
                    )
                    .into_any()
            }
        }
    }
}

use super::*;

impl ThreadView {
    pub(super) fn render_permission_buttons(
        &self,
        session_id: acp::SessionId,
        is_first: bool,
        options: &PermissionOptions,
        entry_ix: usize,
        tool_call_id: acp::ToolCallId,
        focus_handle: &FocusHandle,
        cx: &Context<Self>,
    ) -> Div {
        match options {
            PermissionOptions::Flat(options) => self.render_permission_buttons_flat(
                session_id,
                is_first,
                options,
                entry_ix,
                tool_call_id,
                focus_handle,
                cx,
            ),
            PermissionOptions::Dropdown(choices) => self.render_permission_buttons_with_dropdown(
                is_first,
                choices,
                None,
                entry_ix,
                session_id,
                tool_call_id,
                focus_handle,
                cx,
            ),
            PermissionOptions::DropdownWithPatterns {
                choices,
                patterns,
                tool_name,
            } => self.render_permission_buttons_with_dropdown(
                is_first,
                choices,
                Some((patterns, tool_name)),
                entry_ix,
                session_id,
                tool_call_id,
                focus_handle,
                cx,
            ),
        }
    }

    fn render_permission_buttons_with_dropdown(
        &self,
        is_first: bool,
        choices: &[PermissionOptionChoice],
        patterns: Option<(&[PermissionPattern], &str)>,
        entry_ix: usize,
        session_id: acp::SessionId,
        tool_call_id: acp::ToolCallId,
        focus_handle: &FocusHandle,
        cx: &Context<Self>,
    ) -> Div {
        let selection = self.permission_selections.get(&tool_call_id);

        let selected_index = selection
            .and_then(|s| s.choice_index())
            .unwrap_or_else(|| choices.len().saturating_sub(1));

        let dropdown_label: SharedString =
            if matches!(selection, Some(PermissionSelection::SelectedPatterns(_))) {
                "Always for selected commands".into()
            } else {
                choices
                    .get(selected_index)
                    .or(choices.last())
                    .map(|choice| choice.label())
                    .unwrap_or_else(|| "Only this time".into())
            };

        let dropdown = if let Some((pattern_list, tool_name)) = patterns {
            self.render_permission_granularity_dropdown_with_patterns(
                choices,
                pattern_list,
                tool_name,
                dropdown_label,
                entry_ix,
                tool_call_id.clone(),
                is_first,
                cx,
            )
        } else {
            self.render_permission_granularity_dropdown(
                choices,
                dropdown_label,
                entry_ix,
                tool_call_id.clone(),
                selected_index,
                is_first,
                cx,
            )
        };

        h_flex()
            .w_full()
            .p_1()
            .gap_2()
            .justify_between()
            .border_t_1()
            .border_color(self.tool_card_border_color(cx))
            .child(
                h_flex()
                    .gap_0p5()
                    .child(
                        Button::new(("allow-btn", entry_ix), "Allow")
                            .start_icon(
                                Icon::new(IconName::Check)
                                    .size(IconSize::XSmall)
                                    .color(Color::Success),
                            )
                            .label_size(LabelSize::Small)
                            .when(is_first, |this| {
                                this.key_binding(
                                    KeyBinding::for_action_in(
                                        &AllowOnce as &dyn Action,
                                        focus_handle,
                                        cx,
                                    )
                                    .map(|kb| kb.size(rems_from_px(12.))),
                                )
                            })
                            .on_click(cx.listener({
                                let session_id = session_id.clone();
                                let tool_call_id = tool_call_id.clone();
                                move |this, _, window, cx| {
                                    this.authorize_with_granularity(
                                        session_id.clone(),
                                        tool_call_id.clone(),
                                        true,
                                        window,
                                        cx,
                                    );
                                }
                            })),
                    )
                    .child(
                        Button::new(("deny-btn", entry_ix), "Deny")
                            .start_icon(
                                Icon::new(IconName::Close)
                                    .size(IconSize::XSmall)
                                    .color(Color::Error),
                            )
                            .label_size(LabelSize::Small)
                            .when(is_first, |this| {
                                this.key_binding(
                                    KeyBinding::for_action_in(
                                        &RejectOnce as &dyn Action,
                                        focus_handle,
                                        cx,
                                    )
                                    .map(|kb| kb.size(rems_from_px(12.))),
                                )
                            })
                            .on_click(cx.listener({
                                move |this, _, window, cx| {
                                    this.authorize_with_granularity(
                                        session_id.clone(),
                                        tool_call_id.clone(),
                                        false,
                                        window,
                                        cx,
                                    );
                                }
                            })),
                    ),
            )
            .child(dropdown)
    }

    fn render_permission_buttons_flat(
        &self,
        session_id: acp::SessionId,
        is_first: bool,
        options: &[acp::PermissionOption],
        entry_ix: usize,
        tool_call_id: acp::ToolCallId,
        focus_handle: &FocusHandle,
        cx: &Context<Self>,
    ) -> Div {
        let mut seen_kinds: ArrayVec<acp::PermissionOptionKind, 3, u8> = ArrayVec::new();

        div()
            .p_1()
            .border_t_1()
            .border_color(self.tool_card_border_color(cx))
            .w_full()
            .v_flex()
            .gap_0p5()
            .children(options.iter().map(move |option| {
                let option_id = SharedString::from(option.option_id.0.clone());
                Button::new((option_id, entry_ix), option.name.clone())
                    .map(|this| {
                        let is_retry = option.option_id.0.as_ref()
                            == acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID;
                        let (icon, action) = if is_retry {
                            (
                                Icon::new(IconName::RotateCcw)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                                None,
                            )
                        } else {
                            match option.kind {
                                acp::PermissionOptionKind::AllowOnce => (
                                    Icon::new(IconName::Check)
                                        .size(IconSize::XSmall)
                                        .color(Color::Success),
                                    Some(&AllowOnce as &dyn Action),
                                ),
                                acp::PermissionOptionKind::AllowAlways => (
                                    Icon::new(IconName::CheckDouble)
                                        .size(IconSize::XSmall)
                                        .color(Color::Success),
                                    if option.option_id.0.as_ref()
                                        == acp_thread::SandboxPermission::AllowThread.as_id()
                                    {
                                        None
                                    } else {
                                        Some(&AllowAlways as &dyn Action)
                                    },
                                ),
                                acp::PermissionOptionKind::RejectOnce => (
                                    Icon::new(IconName::Close)
                                        .size(IconSize::XSmall)
                                        .color(Color::Error),
                                    Some(&RejectOnce as &dyn Action),
                                ),
                                acp::PermissionOptionKind::RejectAlways | _ => (
                                    Icon::new(IconName::Close)
                                        .size(IconSize::XSmall)
                                        .color(Color::Error),
                                    None,
                                ),
                            }
                        };

                        let this = this.start_icon(icon);

                        let Some(action) = action else {
                            return this;
                        };

                        if !is_first || seen_kinds.contains(&option.kind) {
                            return this;
                        }

                        seen_kinds.push(option.kind).unwrap();

                        this.key_binding(
                            KeyBinding::for_action_in(action, focus_handle, cx)
                                .map(|kb| kb.size(rems_from_px(12.))),
                        )
                    })
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener({
                        let tool_call_id = tool_call_id.clone();
                        let option_id = option.option_id.clone();
                        let option_kind = option.kind;
                        let session_id = session_id.clone();
                        move |this, _, window, cx| {
                            this.authorize_tool_call(
                                session_id.clone(),
                                tool_call_id.clone(),
                                SelectedPermissionOutcome::new(option_id.clone(), option_kind),
                                window,
                                cx,
                            );
                        }
                    }))
            }))
    }
}

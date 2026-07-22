use super::*;

impl BranchListDelegate {
    fn render_branch_footer(&self, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        if self.is_select_only()
            || !self.show_footer
            || self.editor_position() == PickerEditorPosition::End
        {
            return None;
        }
        let focus_handle = self.focus_handle.clone();

        let footer_container = || {
            h_flex()
                .w_full()
                .p_1p5()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
        };

        match self.state {
            PickerState::List => {
                let selected_entry = self.matches.get(self.selected_index);

                let branch_from_default_button = self
                    .default_branch
                    .as_ref()
                    .filter(|_| matches!(selected_entry, Some(Entry::NewBranch { .. })))
                    .map(|default_branch| {
                        let button_label = format!("Create New From: {default_branch}");

                        Button::new("branch-from-default", button_label)
                            .key_binding(
                                KeyBinding::for_action_in(
                                    &menu::SecondaryConfirm,
                                    &focus_handle,
                                    cx,
                                )
                                .map(|kb| kb.size(rems_from_px(12.))),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delegate.confirm(true, window, cx);
                            }))
                    });

                let delete_and_select_btns = h_flex()
                    .gap_1()
                    .when(
                        !selected_entry
                            .and_then(|entry| entry.as_branch())
                            .is_some_and(|branch| branch.is_head),
                        |this| {
                            this.child(
                                Button::new("delete-branch", "Delete")
                                    .key_binding(
                                        KeyBinding::for_action_in(
                                            &branch_picker::DeleteBranch,
                                            &focus_handle,
                                            cx,
                                        )
                                        .map(|kb| kb.size(rems_from_px(12.))),
                                    )
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(
                                            branch_picker::DeleteBranch.boxed_clone(),
                                            cx,
                                        );
                                    }),
                            )
                        },
                    )
                    .child(
                        Button::new("switch_branch", "Switch")
                            .key_binding(
                                KeyBinding::for_action_in(&menu::Confirm, &focus_handle, cx)
                                    .map(|kb| kb.size(rems_from_px(12.))),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delegate.confirm(false, window, cx);
                            })),
                    );

                Some(
                    footer_container()
                        .map(|this| {
                            if branch_from_default_button.is_some() {
                                this.justify_end().when_some(
                                    branch_from_default_button,
                                    |this, button| {
                                        this.child(button).child(
                                            Button::new("create", "Create")
                                                .key_binding(
                                                    KeyBinding::for_action_in(
                                                        &menu::Confirm,
                                                        &focus_handle,
                                                        cx,
                                                    )
                                                    .map(|kb| kb.size(rems_from_px(12.))),
                                                )
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.delegate.confirm(false, window, cx);
                                                })),
                                        )
                                    },
                                )
                            } else {
                                this.justify_between()
                                    .child({
                                        let focus_handle = focus_handle.clone();
                                        let filter_label = match self.branch_filter {
                                            BranchFilter::All => "Filter Remote",
                                            BranchFilter::Remote => "Show All",
                                        };
                                        Button::new("filter-remotes", filter_label)
                                            .toggle_state(matches!(
                                                self.branch_filter,
                                                BranchFilter::Remote
                                            ))
                                            .key_binding(
                                                KeyBinding::for_action_in(
                                                    &branch_picker::FilterRemotes,
                                                    &focus_handle,
                                                    cx,
                                                )
                                                .map(|kb| kb.size(rems_from_px(12.))),
                                            )
                                            .on_click(|_click, window, cx| {
                                                window.dispatch_action(
                                                    branch_picker::FilterRemotes.boxed_clone(),
                                                    cx,
                                                );
                                            })
                                    })
                                    .child(delete_and_select_btns)
                            }
                        })
                        .into_any_element(),
                )
            }
            PickerState::NewBranch => {
                let branch_from_default_button =
                    self.default_branch.as_ref().map(|default_branch| {
                        let button_label = format!("Create New From: {default_branch}");

                        Button::new("branch-from-default", button_label)
                            .key_binding(
                                KeyBinding::for_action_in(
                                    &menu::SecondaryConfirm,
                                    &focus_handle,
                                    cx,
                                )
                                .map(|kb| kb.size(rems_from_px(12.))),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delegate.confirm(true, window, cx);
                            }))
                    });

                Some(
                    footer_container()
                        .gap_1()
                        .justify_end()
                        .when_some(branch_from_default_button, |this, button| {
                            this.child(button)
                        })
                        .child(
                            Button::new("create-new-branch", "Create")
                                .key_binding(
                                    KeyBinding::for_action_in(&menu::Confirm, &focus_handle, cx)
                                        .map(|kb| kb.size(rems_from_px(12.))),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.delegate.confirm(false, window, cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
            PickerState::CreateRemote(_) => Some(
                footer_container()
                    .justify_end()
                    .child(
                        Button::new("confirm-create-remote", "Confirm")
                            .key_binding(
                                KeyBinding::for_action_in(&menu::Confirm, &focus_handle, cx)
                                    .map(|kb| kb.size(rems_from_px(12.))),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delegate.confirm(false, window, cx);
                            }))
                            .disabled(self.last_query.is_empty()),
                    )
                    .into_any_element(),
            ),
            PickerState::NewRemote => None,
        }
    }
}

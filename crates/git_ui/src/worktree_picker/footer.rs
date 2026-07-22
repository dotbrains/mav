use super::*;

impl WorktreePickerDelegate {
    fn render_footer_impl(
        &self,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        if !self.show_footer {
            return None;
        }

        let focus_handle = self.focus_handle.clone();
        let selected_entry = self.matches.get(self.selected_index);

        let is_creating = selected_entry.is_some_and(|e| {
            matches!(
                e,
                WorktreeEntry::CreateFromCurrentBranch
                    | WorktreeEntry::CreateFromDefaultBranch { .. }
                    | WorktreeEntry::CreateNamed { .. }
            )
        });

        let is_existing_worktree =
            selected_entry.is_some_and(|e| matches!(e, WorktreeEntry::Worktree { .. }));

        let can_delete = selected_entry.is_some_and(|e| {
            matches!(e, WorktreeEntry::Worktree { worktree, .. } if self.can_delete_worktree(worktree))
        });

        let is_current = selected_entry.is_some_and(|e| {
            matches!(e, WorktreeEntry::Worktree { worktree, .. } if self.project_worktree_paths.contains(&worktree.path))
        });

        let is_deleting = selected_entry.is_some_and(|e| {
            matches!(e, WorktreeEntry::Worktree { worktree, .. } if self.deleting_worktree_paths.contains(&worktree.path))
        });

        let footer = h_flex()
            .w_full()
            .p_1p5()
            .gap_0p5()
            .justify_end()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant);

        if is_creating {
            Some(
                footer
                    .child(
                        Button::new("create-worktree", "Create")
                            .key_binding(
                                KeyBinding::for_action_in(&menu::Confirm, &focus_handle, cx)
                                    .map(|kb| kb.size(rems_from_px(12.))),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                            }),
                    )
                    .into_any(),
            )
        } else if is_existing_worktree {
            Some(
                footer
                    .when(is_deleting, |this| {
                        this.child(
                            Button::new("delete-worktree", "Deleting…")
                                .loading(true)
                                .disabled(true),
                        )
                    })
                    .when(!is_deleting && can_delete, |this| {
                        let focus_handle = focus_handle.clone();
                        this.child(
                            Button::new("delete-worktree", "Delete")
                                .key_binding(
                                    KeyBinding::for_action_in(&DeleteWorktree, &focus_handle, cx)
                                        .map(|kb| kb.size(rems_from_px(12.))),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(DeleteWorktree.boxed_clone(), cx)
                                }),
                        )
                    })
                    .when(!is_deleting && !is_current, |this| {
                        let focus_handle = focus_handle.clone();
                        this.child(
                            Button::new("open-in-new-window", "Open in New Window")
                                .key_binding(
                                    KeyBinding::for_action_in(
                                        &menu::SecondaryConfirm,
                                        &focus_handle,
                                        cx,
                                    )
                                    .map(|kb| kb.size(rems_from_px(12.))),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::SecondaryConfirm.boxed_clone(), cx)
                                }),
                        )
                    })
                    .when(!is_deleting, |this| {
                        this.child(
                            Button::new("open-worktree", "Open")
                                .key_binding(
                                    KeyBinding::for_action_in(&menu::Confirm, &focus_handle, cx)
                                        .map(|kb| kb.size(rems_from_px(12.))),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(menu::Confirm.boxed_clone(), cx)
                                }),
                        )
                    })
                    .into_any(),
            )
        } else {
            None
        }
    }
}

use super::*;

impl WorktreePickerDelegate {
    fn confirm_impl(
        &mut self,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(entry) = self.matches.get(self.selected_index) else {
            return;
        };

        match entry {
            WorktreeEntry::Separator | WorktreeEntry::SectionHeader(_) => return,
            WorktreeEntry::CreateFromCurrentBranch => {
                if self.creation_blocked_reason(cx).is_some() {
                    return;
                }
                if let Some(workspace) = self.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        crate::worktree_service::handle_create_worktree(
                            workspace,
                            &CreateWorktree {
                                worktree_name: None,
                                branch_target: NewWorktreeBranchTarget::CurrentBranch,
                            },
                            window,
                            self.focused_dock,
                            cx,
                        );
                    });
                }
            }
            WorktreeEntry::CreateFromDefaultBranch { default_branch } => {
                if self.creation_blocked_reason(cx).is_some() {
                    return;
                }
                if let Some(workspace) = self.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        crate::worktree_service::handle_create_worktree(
                            workspace,
                            &CreateWorktree {
                                worktree_name: None,
                                branch_target: NewWorktreeBranchTarget::RemoteBranch {
                                    remote_name: default_branch.remote_name.clone(),
                                    branch_name: default_branch.branch_name.clone(),
                                },
                            },
                            window,
                            self.focused_dock,
                            cx,
                        );
                    });
                }
            }
            WorktreeEntry::Worktree { worktree, .. } => {
                if self.deleting_worktree_paths.contains(&worktree.path) {
                    return;
                }

                let is_current = self.active_worktree_paths.contains(&worktree.path);

                if !is_current {
                    if secondary {
                        window.dispatch_action(
                            Box::new(OpenWorktreeInNewWindow {
                                path: worktree.path.clone(),
                            }),
                            cx,
                        );
                    } else {
                        let main_worktree_path = self
                            .all_worktrees
                            .iter()
                            .find(|wt| wt.is_main)
                            .map(|wt| wt.path.as_path());
                        if let Some(workspace) = self.workspace.upgrade() {
                            workspace.update(cx, |workspace, cx| {
                                crate::worktree_service::handle_switch_worktree(
                                    workspace,
                                    &SwitchWorktree {
                                        path: worktree.path.clone(),
                                        display_name: worktree.directory_name(main_worktree_path),
                                    },
                                    window,
                                    self.focused_dock,
                                    cx,
                                );
                            });
                        }
                    }
                }
            }
            WorktreeEntry::CreateNamed {
                name,
                from_branch,
                disabled_reason: None,
            } => {
                let branch_target = match from_branch {
                    Some(branch) => NewWorktreeBranchTarget::RemoteBranch {
                        remote_name: branch.remote_name.clone(),
                        branch_name: branch.branch_name.clone(),
                    },
                    None => NewWorktreeBranchTarget::CurrentBranch,
                };
                if let Some(workspace) = self.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        crate::worktree_service::handle_create_worktree(
                            workspace,
                            &CreateWorktree {
                                worktree_name: Some(name.clone()),
                                branch_target,
                            },
                            window,
                            self.focused_dock,
                            cx,
                        );
                    });
                }
            }
            WorktreeEntry::CreateNamed {
                disabled_reason: Some(_),
                ..
            } => {
                return;
            }
        }

        cx.emit(DismissEvent);
    }
}

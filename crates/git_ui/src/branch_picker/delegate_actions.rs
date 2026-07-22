use super::*;

impl BranchListDelegate {
    fn new(
        workspace: WeakEntity<Workspace>,
        repo: Option<Entity<Repository>>,
        style: BranchListStyle,
        branch_selection_behavior: BranchSelectionBehavior,
        cx: &mut Context<BranchList>,
    ) -> Self {
        let restore_selected_branch = match &branch_selection_behavior {
            BranchSelectionBehavior::Checkout => None,
            BranchSelectionBehavior::Select {
                selected_branch, ..
            } => selected_branch.clone(),
        };

        Self {
            workspace,
            matches: vec![],
            repo,
            style,
            all_branches: Vec::new(),
            branch_list_error: None,
            default_branch: None,
            selected_index: 0,
            last_query: Default::default(),
            modifiers: Default::default(),
            branch_filter: BranchFilter::All,
            state: PickerState::List,
            branch_selection_behavior,
            focus_handle: cx.focus_handle(),
            restore_selected_branch,
            show_footer: false,
            hovered_delete_index: None,
        }
    }

    fn is_select_only(&self) -> bool {
        self.branch_selection_behavior.is_select_only()
    }

    fn is_force_delete_hovering_index(&self, index: usize) -> bool {
        self.modifiers.alt && self.hovered_delete_index == Some(index)
    }

    fn create_branch(
        &self,
        from_branch: Option<SharedString>,
        new_branch_name: SharedString,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        let new_branch_name = normalize_branch_name(&new_branch_name);
        let base_branch = from_branch.map(|b| b.to_string());
        cx.spawn(async move |_, cx| {
            repo.update(cx, |repo, _| {
                repo.create_branch(new_branch_name, base_branch)
            })
            .await??;

            Ok(())
        })
        .detach_and_prompt_err("Failed to create branch", window, cx, |e, _, _| {
            Some(e.to_string())
        });
        cx.emit(DismissEvent);
    }

    fn create_remote(
        &self,
        remote_name: String,
        remote_url: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(repo) = self.repo.clone() else {
            return;
        };

        let receiver = repo.update(cx, |repo, _| repo.create_remote(remote_name, remote_url));

        cx.background_spawn(async move { receiver.await? })
            .detach_and_prompt_err("Failed to create remote", window, cx, |e, _, _cx| {
                Some(e.to_string())
            });
        cx.emit(DismissEvent);
    }

    fn delete_at(
        &self,
        idx: usize,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(entry) = self.matches.get(idx).cloned() else {
            return;
        };
        let Some(repo) = self.repo.clone() else {
            return;
        };

        let workspace = self.workspace.clone();

        cx.spawn_in(window, async move |picker, cx| {
            let Entry::Branch { branch, .. } = &entry else {
                log::error!("Failed to delete entry: wrong entry to delete");
                return Ok(());
            };

            if branch.is_head {
                return Ok(());
            }

            let is_remote = branch.is_remote();
            let branch_name = branch.name().to_string();
            let initial_result = repo
                .update(cx, |repo, _| {
                    repo.delete_branch(is_remote, branch_name.clone(), force)
                })
                .await?;

            let (result, attempted_force) = match initial_result {
                Ok(()) => (Ok(()), force),
                Err(error) => {
                    if is_remote {
                        log::error!("Failed to delete remote branch: {error}");
                    } else {
                        log::error!("Failed to delete branch: {error}");
                    }

                    let force_delete_prompt = (!force)
                        .then(|| force_delete_prompt_for_branch_delete_error(&error, entry.name()))
                        .flatten();

                    if let Some(prompt_message) = force_delete_prompt {
                        let answer = cx.update(|window, cx| {
                            window.prompt(
                                PromptLevel::Warning,
                                &prompt_message,
                                None,
                                &["Force Delete", "Cancel"],
                                cx,
                            )
                        })?;

                        if answer.await != Ok(0) {
                            return Ok(());
                        }

                        let retry = repo
                            .update(cx, |repo, _| {
                                repo.delete_branch(is_remote, branch_name, true)
                            })
                            .await?;

                        if let Err(error) = &retry {
                            log::error!("Failed to force delete branch: {error}");
                        }
                        (retry, true)
                    } else {
                        (Err(error), force)
                    }
                }
            };

            if let Err(error) = result {
                if let Some(workspace) = workspace.upgrade() {
                    cx.update(|_window, cx| {
                        show_error_toast(
                            workspace,
                            delete_branch_command(is_remote, entry.name(), attempted_force),
                            error,
                            cx,
                        )
                    })?;
                }

                return Ok(());
            }

            picker.update_in(cx, |picker, _, cx| {
                picker.delegate.matches.retain(|e| e != &entry);

                if let Entry::Branch { branch, .. } = &entry {
                    picker
                        .delegate
                        .all_branches
                        .retain(|e| e.ref_name != branch.ref_name);
                }

                if picker.delegate.matches.is_empty() {
                    picker.delegate.selected_index = 0;
                } else if picker.delegate.selected_index >= picker.delegate.matches.len() {
                    picker.delegate.selected_index = picker.delegate.matches.len() - 1;
                }

                picker.delegate.hovered_delete_index = None;

                cx.notify();
            })?;

            anyhow::Ok(())
        })
        .detach();
    }
}

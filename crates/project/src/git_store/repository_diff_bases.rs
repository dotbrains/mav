use super::*;

impl Repository {
    pub(super) fn reload_buffer_diff_bases(&mut self, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let git_store = self.git_store.clone();
        let _ = self.send_keyed_job(
            "reload_buffer_diff_bases",
            Some(GitJobKey::ReloadBufferDiffBases),
            None,
            |state, mut cx| async move {
                let RepositoryState::Local(LocalRepositoryState { backend, .. }) = state else {
                    log::error!("tried to recompute diffs for a non-local repository");
                    return Ok(());
                };

                let Some(this) = this.upgrade() else {
                    return Ok(());
                };

                let repo_diff_state_updates = this.update(&mut cx, |this, cx| {
                    git_store.update(cx, |git_store, cx| {
                        git_store
                            .diffs
                            .iter()
                            .filter_map(|(buffer_id, diff_state)| {
                                let buffer_store = git_store.buffer_store.read(cx);
                                let buffer = buffer_store.get(*buffer_id)?;
                                let file = File::from_dyn(buffer.read(cx).file())?;
                                let abs_path = file.worktree.read(cx).absolutize(&file.path);
                                let repo_path = this.abs_path_to_repo_path(&abs_path)?;
                                let is_symlink = GitStore::file_is_symlink(file, cx);
                                log::debug!(
                                    "start reload diff bases for repo path {}",
                                    repo_path.as_unix_str()
                                );
                                diff_state.update(cx, |diff_state, _| {
                                    let has_unstaged_diff = diff_state
                                        .unstaged_diff
                                        .as_ref()
                                        .is_some_and(|diff| diff.is_upgradable());
                                    let has_staged_diff = diff_state
                                        .staged_diff
                                        .as_ref()
                                        .is_some_and(|(diff, _)| diff.is_upgradable());
                                    let has_uncommitted_diff = diff_state
                                        .uncommitted_diff
                                        .as_ref()
                                        .is_some_and(|set| set.is_upgradable());

                                    Some((
                                        buffer,
                                        repo_path,
                                        is_symlink,
                                        (has_unstaged_diff || has_staged_diff)
                                            .then(|| diff_state.index_text.clone()),
                                        (has_staged_diff || has_uncommitted_diff)
                                            .then(|| diff_state.head_text.clone()),
                                    ))
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                })?;

                let buffer_diff_base_changes = cx
                    .background_spawn(async move {
                        let mut changes = Vec::new();
                        for (
                            buffer,
                            repo_path,
                            is_symlink,
                            current_index_text,
                            current_head_text,
                        ) in &repo_diff_state_updates
                        {
                            let index_text = if current_index_text.is_some() && !*is_symlink {
                                backend.load_index_text(repo_path.clone())
                            } else {
                                future::ready(None).boxed()
                            };
                            let head_text = if current_head_text.is_some() && !*is_symlink {
                                backend.load_committed_text(repo_path.clone())
                            } else {
                                future::ready(None).boxed()
                            };
                            let (index_text, head_text) = future::join(index_text, head_text).await;

                            let change =
                                match (current_index_text.as_ref(), current_head_text.as_ref()) {
                                    (Some(current_index), Some(current_head)) => {
                                        let index_changed =
                                            index_text.as_deref() != current_index.as_deref();
                                        let head_changed =
                                            head_text.as_deref() != current_head.as_deref();
                                        if index_changed && head_changed {
                                            if index_text == head_text {
                                                Some(DiffBasesChange::SetBoth(head_text))
                                            } else {
                                                Some(DiffBasesChange::SetEach {
                                                    index: index_text,
                                                    head: head_text,
                                                })
                                            }
                                        } else if index_changed {
                                            Some(DiffBasesChange::SetIndex(index_text))
                                        } else if head_changed {
                                            Some(DiffBasesChange::SetHead(head_text))
                                        } else {
                                            None
                                        }
                                    }
                                    (Some(current_index), None) => {
                                        let index_changed =
                                            index_text.as_deref() != current_index.as_deref();
                                        index_changed
                                            .then_some(DiffBasesChange::SetIndex(index_text))
                                    }
                                    (None, Some(current_head)) => {
                                        let head_changed =
                                            head_text.as_deref() != current_head.as_deref();
                                        head_changed.then_some(DiffBasesChange::SetHead(head_text))
                                    }
                                    (None, None) => None,
                                };

                            changes.push((buffer.clone(), change))
                        }
                        changes
                    })
                    .await;

                git_store.update(&mut cx, |git_store, cx| {
                    for (buffer, diff_bases_change) in buffer_diff_base_changes {
                        let buffer_snapshot = buffer.read(cx).text_snapshot();
                        let buffer_id = buffer_snapshot.remote_id();
                        let Some(diff_state) = git_store.diffs.get(&buffer_id) else {
                            continue;
                        };

                        let downstream_client = git_store.downstream_client();
                        diff_state.update(cx, |diff_state, cx| {
                            use proto::update_diff_bases::Mode;

                            if let Some((diff_bases_change, (client, project_id))) =
                                diff_bases_change.clone().zip(downstream_client)
                            {
                                let (staged_text, committed_text, mode) = match diff_bases_change {
                                    DiffBasesChange::SetIndex(index) => {
                                        (index, None, Mode::IndexOnly)
                                    }
                                    DiffBasesChange::SetHead(head) => (None, head, Mode::HeadOnly),
                                    DiffBasesChange::SetEach { index, head } => {
                                        (index, head, Mode::IndexAndHead)
                                    }
                                    DiffBasesChange::SetBoth(text) => {
                                        (None, text, Mode::IndexMatchesHead)
                                    }
                                };
                                client
                                    .send(proto::UpdateDiffBases {
                                        project_id: project_id.to_proto(),
                                        buffer_id: buffer_id.to_proto(),
                                        staged_text,
                                        committed_text,
                                        mode: mode as i32,
                                    })
                                    .log_err();
                            }

                            diff_state.diff_bases_changed(buffer_snapshot, diff_bases_change, cx);
                        });
                    }
                })
            },
        );
    }
}

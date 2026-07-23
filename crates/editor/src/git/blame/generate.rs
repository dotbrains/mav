use super::*;

const REGENERATE_ON_EDIT_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);

impl GitBlame {
    #[ztracing::instrument(skip_all)]
    pub(super) fn generate(&mut self, cx: &mut Context<Self>) {
        if !self.focused {
            self.changed_while_blurred = true;
            return;
        }
        let buffers_to_blame = self
            .multi_buffer
            .update(cx, |multi_buffer, cx| {
                let snapshot = multi_buffer.snapshot(cx);
                snapshot
                    .all_buffer_ids()
                    .filter_map(|id| Some(multi_buffer.buffer(id)?.downgrade()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let project = self.project.downgrade();
        self.task = cx.spawn(async move |this, cx| {
            let mut all_results = Vec::new();
            let mut all_errors = Vec::new();

            for buffers in buffers_to_blame.chunks(4) {
                let span = ztracing::debug_span!("for each chunk of buffers");
                let _enter = span.enter();
                let blame = cx.update(|cx| {
                    buffers
                        .iter()
                        .map(|buffer| {
                            let buffer = buffer.upgrade().context("buffer was dropped")?;
                            let project = project.upgrade().context("project was dropped")?;
                            let id = buffer.read(cx).remote_id();
                            let snapshot = buffer.read(cx).snapshot();
                            let buffer_edits = buffer.update(cx, |buffer, _| buffer.subscribe());

                            let repository = project
                                .read(cx)
                                .git_store()
                                .read(cx)
                                .repository_and_path_for_buffer_id(id, cx);

                            let remote_url = repository
                                .as_ref()
                                .and_then(|(repo, _)| repo.read(cx).default_remote_url());

                            let blame_buffer = if repository.is_some() {
                                project.update(cx, |project, cx| {
                                    project.blame_buffer(&buffer, None, cx)
                                })
                            } else {
                                Task::ready(Ok(None))
                            };

                            Ok(async move {
                                (id, snapshot, buffer_edits, blame_buffer.await, remote_url)
                            })
                        })
                        .collect::<Result<Vec<_>>>()
                })?;
                let provider_registry =
                    cx.update(|cx| GitHostingProviderRegistry::default_global(cx));
                let (results, errors) = cx
                    .background_spawn({
                        async move {
                            let blame = futures::future::join_all(blame).await;
                            let mut res = vec![];
                            let mut errors = vec![];
                            for (id, snapshot, buffer_edits, blame, remote_url) in blame {
                                match blame {
                                    Ok(Some(Blame { entries, messages })) => {
                                        let entries = build_blame_entry_sum_tree(
                                            entries,
                                            snapshot.max_point().row,
                                        );
                                        let commit_details = messages
                                            .into_iter()
                                            .map(|(oid, message)| {
                                                let parsed_commit_message =
                                                    ParsedCommitMessage::parse(
                                                        oid.to_string(),
                                                        message,
                                                        remote_url.as_deref(),
                                                        Some(provider_registry.clone()),
                                                    );
                                                (oid, parsed_commit_message)
                                            })
                                            .collect();
                                        res.push((
                                            id,
                                            snapshot,
                                            buffer_edits,
                                            Some(entries),
                                            commit_details,
                                        ));
                                    }
                                    Ok(None) => res.push((
                                        id,
                                        snapshot,
                                        buffer_edits,
                                        None,
                                        Default::default(),
                                    )),
                                    Err(e) => errors.push(e),
                                }
                            }
                            (res, errors)
                        }
                    })
                    .await;
                all_results.extend(results);
                all_errors.extend(errors)
            }

            this.update(cx, |this, cx| {
                this.buffers.clear();
                for (id, snapshot, buffer_edits, entries, commit_details) in all_results {
                    let Some(entries) = entries else {
                        continue;
                    };
                    this.buffers.insert(
                        id,
                        GitBlameBuffer {
                            buffer_edits,
                            buffer_snapshot: snapshot,
                            entries,
                            commit_details,
                        },
                    );
                }
                cx.notify();
                if !all_errors.is_empty() {
                    this.project.update(cx, |_, cx| {
                        let all_errors = all_errors
                            .into_iter()
                            .map(|e| format!("{e:#}"))
                            .dedup()
                            .collect::<Vec<_>>();
                        let all_errors = all_errors.join(", ");
                        if this.user_triggered {
                            log::error!("failed to get git blame data: {all_errors}");
                            cx.emit(project::Event::Toast {
                                notification_id: "git-blame".into(),
                                message: all_errors,
                                link: None,
                            });
                        } else {
                            // If we weren't triggered by a user, we just log errors in the background, instead of sending
                            // notifications.
                            log::debug!("failed to get git blame data: {all_errors}");
                        }
                    })
                }
            })
        });
    }

    pub(super) fn regenerate_on_edit(&mut self, cx: &mut Context<Self>) {
        // todo(lw): hot foreground spawn
        self.regenerate_on_edit_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(REGENERATE_ON_EDIT_DEBOUNCE_INTERVAL)
                .await;

            this.update(cx, |this, cx| {
                this.generate(cx);
            })
        });
    }
}

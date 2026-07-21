use super::*;

impl GitStore {
    pub(super) fn file_is_symlink(file: &File, cx: &App) -> bool {
        file.worktree
            .read(cx)
            .entry_for_path(&file.path)
            .is_some_and(|entry| entry.canonical_path.is_some())
    }

    pub(super) fn buffer_is_symlink(buffer: &Entity<Buffer>, cx: &App) -> bool {
        File::from_dyn(buffer.read(cx).file()).is_some_and(|file| Self::file_is_symlink(file, cx))
    }

    pub fn open_unstaged_diff(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<BufferDiff>>> {
        let buffer_id = buffer.read(cx).remote_id();
        if let Some(diff_state) = self.diffs.get(&buffer_id)
            && let Some(unstaged_diff) = diff_state
                .read(cx)
                .unstaged_diff
                .as_ref()
                .and_then(|weak| weak.upgrade())
        {
            if let Some(task) =
                diff_state.update(cx, |diff_state, _| diff_state.wait_for_recalculation())
            {
                return cx.background_executor().spawn(async move {
                    task.await;
                    Ok(unstaged_diff)
                });
            }
            return Task::ready(Ok(unstaged_diff));
        }

        let Some((repo, repo_path)) =
            self.repository_and_path_for_buffer_id(buffer.read(cx).remote_id(), cx)
        else {
            return Task::ready(Err(anyhow!("failed to find git repository for buffer")));
        };

        let is_symlink = Self::buffer_is_symlink(&buffer, cx);
        let task = self
            .loading_diffs
            .entry((buffer_id, DiffKind::Unstaged))
            .or_insert_with(|| {
                let staged_text = if is_symlink {
                    Task::ready(Ok(None))
                } else {
                    repo.update(cx, |repo, cx| {
                        repo.load_staged_text(buffer_id, repo_path, cx)
                    })
                };
                cx.spawn(async move |this, cx| {
                    Self::open_diff_internal(
                        this,
                        DiffKind::Unstaged,
                        staged_text.await.map(DiffBasesChange::SetIndex),
                        buffer,
                        cx,
                    )
                    .await
                    .map_err(Arc::new)
                })
                .shared()
            })
            .clone();

        cx.background_spawn(async move { task.await.map_err(|e| anyhow!("{e}")) })
    }

    pub fn open_staged_diff(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<BufferDiff>>> {
        let buffer_id = buffer.read(cx).remote_id();

        if let Some(diff_state) = self.diffs.get(&buffer_id)
            && let Some(staged_diff) = diff_state.read(cx).staged_diff()
        {
            if let Some(task) =
                diff_state.update(cx, |diff_state, _| diff_state.wait_for_recalculation())
            {
                return cx.background_executor().spawn(async move {
                    task.await;
                    Ok(staged_diff)
                });
            }
            return Task::ready(Ok(staged_diff));
        }

        let Some((repo, repo_path)) =
            self.repository_and_path_for_buffer_id(buffer.read(cx).remote_id(), cx)
        else {
            return Task::ready(Err(anyhow!("failed to find git repository for buffer")));
        };

        let task = self
            .loading_diffs
            .entry((buffer_id, DiffKind::Staged))
            .or_insert_with(|| {
                let changes = repo.update(cx, |repo, cx| {
                    repo.load_committed_text(buffer_id, repo_path, cx)
                });

                cx.spawn(async move |this, cx| {
                    Self::open_diff_internal(this, DiffKind::Staged, changes.await, buffer, cx)
                        .await
                        .map_err(Arc::new)
                })
                .shared()
            })
            .clone();

        cx.background_spawn(async move { task.await.map_err(|e| anyhow!("{e}")) })
    }

    pub fn open_diff_since(
        &mut self,
        oid: Option<git::Oid>,
        buffer: Entity<Buffer>,
        repo: Entity<Repository>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<BufferDiff>>> {
        let buffer_id = buffer.read(cx).remote_id();

        if let Some(diff_state) = self.diffs.get(&buffer_id)
            && let Some(oid_diff) = diff_state.read(cx).oid_diff(oid)
        {
            if let Some(task) =
                diff_state.update(cx, |diff_state, _| diff_state.wait_for_recalculation())
            {
                return cx.background_executor().spawn(async move {
                    task.await;
                    Ok(oid_diff)
                });
            }
            return Task::ready(Ok(oid_diff));
        }

        let diff_kind = DiffKind::SinceOid(oid);
        if let Some(task) = self.loading_diffs.get(&(buffer_id, diff_kind)) {
            let task = task.clone();
            return cx.background_spawn(async move { task.await.map_err(|e| anyhow!("{e}")) });
        }

        let task = cx
            .spawn(async move |this, cx| {
                let result: Result<Entity<BufferDiff>> = async {
                    let buffer_snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
                    let language_registry =
                        buffer.update(cx, |buffer, _| buffer.language_registry());
                    let content: Option<Arc<str>> = match oid {
                        None => None,
                        Some(oid) => Some({
                            let mut content = repo
                                .update(cx, |repo, cx| repo.load_blob_content(oid, cx))
                                .await?;
                            text::LineEnding::normalize(&mut content);
                            content.into()
                        }),
                    };
                    let buffer_diff = cx.new(|cx| {
                        BufferDiff::new(
                            &buffer_snapshot,
                            buffer_snapshot.language().cloned(),
                            language_registry,
                            cx,
                        )
                    });

                    buffer_diff
                        .update(cx, |buffer_diff, cx| {
                            buffer_diff.set_base_text(content.clone(), buffer_snapshot.text, cx)
                        })
                        .await;
                    let unstaged_diff = this
                        .update(cx, |this, cx| this.open_unstaged_diff(buffer.clone(), cx))?
                        .await?;
                    buffer_diff.update(cx, |buffer_diff, _| {
                        buffer_diff.set_secondary_diff(unstaged_diff);
                    });

                    this.update(cx, |this, cx| {
                        cx.subscribe(&buffer_diff, Self::on_buffer_diff_event)
                            .detach();

                        this.loading_diffs.remove(&(buffer_id, diff_kind));

                        let git_store = cx.weak_entity();
                        let diff_state = this
                            .diffs
                            .entry(buffer_id)
                            .or_insert_with(|| cx.new(|cx| BufferGitState::new(git_store, cx)));

                        diff_state.update(cx, |state, _| {
                            if let Some(oid) = oid {
                                if let Some(content) = content {
                                    state.oid_texts.insert(oid, content);
                                }
                            }
                            state.oid_diffs.insert(oid, buffer_diff.downgrade());
                        });
                    })?;

                    Ok(buffer_diff)
                }
                .await;
                result.map_err(Arc::new)
            })
            .shared();

        self.loading_diffs
            .insert((buffer_id, diff_kind), task.clone());
        cx.background_spawn(async move { task.await.map_err(|e| anyhow!("{e}")) })
    }

    #[ztracing::instrument(skip_all)]
    pub fn open_uncommitted_diff(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<BufferDiff>>> {
        let buffer_id = buffer.read(cx).remote_id();

        if let Some(diff_state) = self.diffs.get(&buffer_id)
            && let Some(uncommitted_diff) = diff_state
                .read(cx)
                .uncommitted_diff
                .as_ref()
                .and_then(|weak| weak.upgrade())
        {
            if let Some(task) =
                diff_state.update(cx, |diff_state, _| diff_state.wait_for_recalculation())
            {
                return cx.background_executor().spawn(async move {
                    task.await;
                    Ok(uncommitted_diff)
                });
            }
            return Task::ready(Ok(uncommitted_diff));
        }

        let Some((repo, repo_path)) =
            self.repository_and_path_for_buffer_id(buffer.read(cx).remote_id(), cx)
        else {
            return Task::ready(Err(anyhow!("failed to find git repository for buffer")));
        };

        let is_symlink = Self::buffer_is_symlink(&buffer, cx);
        let task = self
            .loading_diffs
            .entry((buffer_id, DiffKind::Uncommitted))
            .or_insert_with(|| {
                let changes = if is_symlink {
                    Task::ready(Ok(DiffBasesChange::SetBoth(None)))
                } else {
                    repo.update(cx, |repo, cx| {
                        repo.load_committed_text(buffer_id, repo_path, cx)
                    })
                };

                // todo(lw): hot foreground spawn
                cx.spawn(async move |this, cx| {
                    Self::open_diff_internal(this, DiffKind::Uncommitted, changes.await, buffer, cx)
                        .await
                        .map_err(Arc::new)
                })
                .shared()
            })
            .clone();

        cx.background_spawn(async move { task.await.map_err(|e| anyhow!("{e}")) })
    }

    #[ztracing::instrument(skip_all)]
    async fn open_diff_internal(
        this: WeakEntity<Self>,
        kind: DiffKind,
        texts: Result<DiffBasesChange>,
        buffer_entity: Entity<Buffer>,
        cx: &mut AsyncApp,
    ) -> Result<Entity<BufferDiff>> {
        let diff_bases_change = match texts {
            Err(e) => {
                this.update(cx, |this, cx| {
                    let buffer = buffer_entity.read(cx);
                    let buffer_id = buffer.remote_id();
                    this.loading_diffs.remove(&(buffer_id, kind));
                })?;
                return Err(e);
            }
            Ok(change) => change,
        };

        this.update(cx, |this, cx| {
            let buffer = buffer_entity.read(cx);
            let buffer_id = buffer.remote_id();
            let language = buffer.language().cloned();
            let language_registry = buffer.language_registry();
            let text_snapshot = buffer.text_snapshot();
            this.loading_diffs.remove(&(buffer_id, kind));

            let git_store = cx.weak_entity();
            let diff_state = this
                .diffs
                .entry(buffer_id)
                .or_insert_with(|| cx.new(|cx| BufferGitState::new(git_store, cx)));

            let existing_unstaged_diff = diff_state.read(cx).unstaged_diff();

            let mut staged_index_text_buffer = None;
            let diff = if kind == DiffKind::Unstaged
                && let Some(existing_unstaged_diff) = existing_unstaged_diff.clone()
            {
                existing_unstaged_diff
            } else {
                let diff = match kind {
                    DiffKind::Unstaged => {
                        let base_text_buffer = diff_state.update(cx, |diff_state, cx| {
                            diff_state.get_or_create_index_text_buffer(cx)
                        });
                        cx.new(|cx| {
                            BufferDiff::new_with_base_text_buffer(
                                &text_snapshot,
                                base_text_buffer,
                                cx,
                            )
                        })
                    }
                    DiffKind::Staged => {
                        let (index_text_buffer, base_text_buffer) =
                            diff_state.update(cx, |diff_state, cx| {
                                (
                                    diff_state.get_or_create_index_text_buffer(cx),
                                    diff_state.get_or_create_head_text_buffer(cx),
                                )
                            });
                        index_text_buffer.update(cx, |index_text_buffer, cx| {
                            if let Some(language_registry) = language_registry.clone() {
                                index_text_buffer.set_language_registry(language_registry);
                            }
                            index_text_buffer.set_language_async(language.clone(), cx);
                        });
                        let index_text_snapshot = index_text_buffer.read(cx).text_snapshot();
                        staged_index_text_buffer = Some(index_text_buffer);
                        cx.new(|cx| {
                            BufferDiff::new_with_base_text_buffer(
                                &index_text_snapshot,
                                base_text_buffer,
                                cx,
                            )
                        })
                    }
                    DiffKind::Uncommitted => {
                        let base_text_buffer = diff_state.update(cx, |diff_state, cx| {
                            diff_state.get_or_create_head_text_buffer(cx)
                        });
                        cx.new(|cx| {
                            BufferDiff::new_with_base_text_buffer(
                                &text_snapshot,
                                base_text_buffer,
                                cx,
                            )
                        })
                    }
                    DiffKind::SinceOid(_) => {
                        unreachable!("open_diff_internal is not used for OID diffs")
                    }
                };
                cx.subscribe(&diff, Self::on_buffer_diff_event).detach();
                diff
            };
            diff_state.update(cx, |diff_state, cx| {
                diff_state.language = language;
                diff_state.language_registry = language_registry;

                match kind {
                    DiffKind::Unstaged => {
                        diff_state.unstaged_diff = Some(diff.downgrade());
                    }
                    DiffKind::Staged => {
                        diff_state.index_text_buffer_language_enabled = true;
                        let index_text_buffer = staged_index_text_buffer
                            .take()
                            .context("index text buffer was not created for staged diff")?;
                        diff_state.staged_diff = Some((diff.downgrade(), index_text_buffer));
                    }
                    DiffKind::Uncommitted => {
                        let unstaged_diff = if let Some(diff) = existing_unstaged_diff {
                            diff
                        } else {
                            let base_text_buffer = diff_state.get_or_create_index_text_buffer(cx);
                            let unstaged_diff = cx.new(|cx| {
                                BufferDiff::new_with_base_text_buffer(
                                    &text_snapshot,
                                    base_text_buffer,
                                    cx,
                                )
                            });
                            diff_state.unstaged_diff = Some(unstaged_diff.downgrade());
                            unstaged_diff
                        };

                        diff.update(cx, |diff, _| diff.set_secondary_diff(unstaged_diff));
                        diff_state.uncommitted_diff = Some(diff.downgrade())
                    }
                    DiffKind::SinceOid(_) => {
                        unreachable!("open_diff_internal is not used for OID diffs")
                    }
                }

                diff_state.diff_bases_changed(text_snapshot, Some(diff_bases_change), cx);
                let rx = diff_state.wait_for_recalculation();

                anyhow::Ok(async move {
                    if let Some(rx) = rx {
                        rx.await;
                    }
                    Ok(diff)
                })
            })
        })??
        .await
    }

    pub fn get_unstaged_diff(&self, buffer_id: BufferId, cx: &App) -> Option<Entity<BufferDiff>> {
        let diff_state = self.diffs.get(&buffer_id)?;
        diff_state.read(cx).unstaged_diff.as_ref()?.upgrade()
    }

    pub fn get_staged_diff(&self, buffer_id: BufferId, cx: &App) -> Option<Entity<BufferDiff>> {
        let diff_state = self.diffs.get(&buffer_id)?;
        diff_state.read(cx).staged_diff()
    }

    pub fn get_uncommitted_diff(
        &self,
        buffer_id: BufferId,
        cx: &App,
    ) -> Option<Entity<BufferDiff>> {
        let diff_state = self.diffs.get(&buffer_id)?;
        diff_state.read(cx).uncommitted_diff.as_ref()?.upgrade()
    }

    pub fn get_diff_since_oid(
        &self,
        buffer_id: BufferId,
        oid: Option<git::Oid>,
        cx: &App,
    ) -> Option<Entity<BufferDiff>> {
        let diff_state = self.diffs.get(&buffer_id)?;
        diff_state.read(cx).oid_diff(oid)
    }

    /// Whether this buffer's index text is known to match its committed text
    /// without comparing contents, i.e. whether the texts share one allocation.
    /// In a downstream project, this can only be true if the upstream sent
    /// `Mode::IndexMatchesHead`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn index_matches_head_for_buffer(&self, buffer_id: BufferId, cx: &App) -> bool {
        self.diffs
            .get(&buffer_id)
            .is_some_and(|diff_state| diff_state.read(cx).index_matches_head())
    }
}

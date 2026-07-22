use super::*;

impl ThreadMetadataStore {
    pub fn update_working_directories(
        &mut self,
        thread_id: ThreadId,
        work_dirs: PathList,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.threads.get(&thread_id) {
            debug_assert!(
                !thread.archived,
                "update_working_directories called on archived thread"
            );
            self.save_internal(ThreadMetadata {
                worktree_paths: WorktreePaths::from_path_lists(
                    thread.main_worktree_paths().clone(),
                    work_dirs.clone(),
                )
                .unwrap_or_else(|_| WorktreePaths::from_folder_paths(&work_dirs)),
                ..thread.clone()
            });
            cx.notify();
        }
    }

    pub fn update_worktree_paths(
        &mut self,
        thread_ids: &[ThreadId],
        worktree_paths: WorktreePaths,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for &thread_id in thread_ids {
            let Some(thread) = self.threads.get(&thread_id) else {
                continue;
            };
            if thread.worktree_paths == worktree_paths {
                continue;
            }
            // Don't overwrite paths for archived threads — the
            // project may no longer include the worktree that was
            // removed during the archive flow.
            if thread.archived {
                continue;
            }
            self.save_internal(ThreadMetadata {
                worktree_paths: worktree_paths.clone(),
                ..thread.clone()
            });
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    pub fn update_interacted_at(
        &mut self,
        thread_id: &ThreadId,
        time: DateTime<Utc>,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.threads.get(thread_id) {
            self.save_internal(ThreadMetadata {
                interacted_at: Some(time),
                ..thread.clone()
            });
            cx.notify();
        };
    }

    pub fn archive(
        &mut self,
        thread_id: ThreadId,
        archive_job: Option<(Task<()>, async_channel::Sender<()>)>,
        cx: &mut Context<Self>,
    ) {
        self.update_archived(thread_id, true, cx);

        if let Some(job) = archive_job {
            self.in_flight_archives.insert(thread_id, job);
        }

        cx.emit(ThreadMetadataStoreEvent::ThreadArchived(thread_id));
    }

    pub fn unarchive(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        self.update_archived(thread_id, false, cx);
        // Dropping the Sender triggers cancellation in the background task.
        self.in_flight_archives.remove(&thread_id);
    }

    pub fn cleanup_completed_archive(&mut self, thread_id: ThreadId) {
        self.in_flight_archives.remove(&thread_id);
    }

    /// Returns `true` if any unarchived thread other than `thread_id`
    /// references `path` in its folder paths. Used to determine whether a
    /// worktree can safely be removed from disk.
    pub fn path_is_referenced_by_unarchived_threads(
        &self,
        thread_id: Option<ThreadId>,
        path: &Path,
        remote_connection: Option<&RemoteConnectionOptions>,
    ) -> bool {
        self.path_is_referenced_by_unarchived_threads_matching(
            thread_id,
            path,
            remote_connection,
            |_| true,
        )
    }

    pub fn path_is_referenced_by_unarchived_threads_matching(
        &self,
        thread_id: Option<ThreadId>,
        path: &Path,
        remote_connection: Option<&RemoteConnectionOptions>,
        matches: impl Fn(&ThreadMetadata) -> bool,
    ) -> bool {
        self.entries().any(|thread| {
            Some(thread.thread_id) != thread_id
                && !thread.archived
                && thread.matches_remote_connection(remote_connection)
                && thread.references_folder_path(path)
                && matches(thread)
        })
    }

    /// Updates a thread's `folder_paths` after an archived worktree has been
    /// restored to disk. The restored worktree may land at a different path
    /// than it had before archival, so each `(old_path, new_path)` pair in
    /// `path_replacements` is applied to the thread's stored folder paths.
    pub fn update_restored_worktree_paths(
        &mut self,
        thread_id: ThreadId,
        path_replacements: &[(PathBuf, PathBuf)],
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.threads.get(&thread_id).cloned() {
            let mut paths: Vec<PathBuf> = thread.folder_paths().paths().to_vec();
            for (old_path, new_path) in path_replacements {
                if let Some(pos) = paths.iter().position(|p| p == old_path) {
                    paths[pos] = new_path.clone();
                }
            }
            let new_folder_paths = PathList::new(&paths);
            self.save_internal(ThreadMetadata {
                worktree_paths: WorktreePaths::from_path_lists(
                    thread.main_worktree_paths().clone(),
                    new_folder_paths.clone(),
                )
                .unwrap_or_else(|_| WorktreePaths::from_folder_paths(&new_folder_paths)),
                ..thread
            });
            cx.notify();
        }
    }

    pub fn complete_worktree_restore(
        &mut self,
        thread_id: ThreadId,
        path_replacements: &[(PathBuf, PathBuf)],
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.threads.get(&thread_id).cloned() {
            let mut paths: Vec<PathBuf> = thread.folder_paths().paths().to_vec();
            for (old_path, new_path) in path_replacements {
                for path in &mut paths {
                    if path == old_path {
                        *path = new_path.clone();
                    }
                }
            }
            let new_folder_paths = PathList::new(&paths);
            self.save_internal(ThreadMetadata {
                worktree_paths: WorktreePaths::from_path_lists(
                    thread.main_worktree_paths().clone(),
                    new_folder_paths.clone(),
                )
                .unwrap_or_else(|_| WorktreePaths::from_folder_paths(&new_folder_paths)),
                ..thread
            });
            cx.notify();
        }
    }

    /// Apply a mutation to the worktree paths of all threads whose current
    /// `folder_paths` matches `current_folder_paths`, then re-index.
    /// When `remote_connection` is provided, only threads with a matching
    /// remote connection are affected.
    pub fn change_worktree_paths(
        &mut self,
        current_folder_paths: &PathList,
        remote_connection: Option<&RemoteConnectionOptions>,
        mutate: impl Fn(&mut WorktreePaths),
        cx: &mut Context<Self>,
    ) {
        let thread_ids: Vec<_> = self
            .threads_by_paths
            .get(current_folder_paths)
            .into_iter()
            .flatten()
            .filter(|id| {
                self.threads.get(id).is_some_and(|t| {
                    !t.archived
                        && same_remote_connection_identity(
                            t.remote_connection.as_ref(),
                            remote_connection,
                        )
                })
            })
            .copied()
            .collect();

        self.mutate_thread_paths(&thread_ids, mutate, cx);
    }

    fn mutate_thread_paths(
        &mut self,
        thread_ids: &[ThreadId],
        mutate: impl Fn(&mut WorktreePaths),
        cx: &mut Context<Self>,
    ) {
        if thread_ids.is_empty() {
            return;
        }

        for thread_id in thread_ids {
            if let Some(thread) = self.threads.get_mut(thread_id) {
                if let Some(ids) = self
                    .threads_by_main_paths
                    .get_mut(thread.main_worktree_paths())
                {
                    ids.remove(thread_id);
                }
                if let Some(ids) = self.threads_by_paths.get_mut(thread.folder_paths()) {
                    ids.remove(thread_id);
                }

                mutate(&mut thread.worktree_paths);

                self.threads_by_main_paths
                    .entry(thread.main_worktree_paths().clone())
                    .or_default()
                    .insert(*thread_id);
                self.threads_by_paths
                    .entry(thread.folder_paths().clone())
                    .or_default()
                    .insert(*thread_id);

                self.pending_thread_ops_tx
                    .try_send(DbOperation::Upsert(thread.clone()))
                    .log_err();
            }
        }

        cx.notify();
    }
}

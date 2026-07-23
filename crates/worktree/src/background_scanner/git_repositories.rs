use super::*;

impl BackgroundScanner {
    pub(super) async fn update_git_repositories(
        &self,
        dot_git_paths: Vec<PathBuf>,
    ) -> Vec<Arc<Path>> {
        log::trace!("reloading repositories: {dot_git_paths:?}");
        let mut state = self.state.lock().await;
        let scan_id = state.snapshot.scan_id;
        let mut affected_repo_roots = Vec::new();
        for dot_git_dir in dot_git_paths {
            let existing_repository_entry =
                state
                    .snapshot
                    .git_repositories
                    .iter()
                    .find_map(|(_, repo)| {
                        let dot_git_dir = SanitizedPath::new(&dot_git_dir);
                        if SanitizedPath::new(repo.common_dir_abs_path.as_ref()) == dot_git_dir
                            || SanitizedPath::new(repo.repository_dir_abs_path.as_ref())
                                == dot_git_dir
                            || SanitizedPath::new(repo.dot_git_abs_path.as_ref()) == dot_git_dir
                        {
                            Some(repo.clone())
                        } else {
                            None
                        }
                    });

            match existing_repository_entry {
                None => {
                    let Ok(relative) = dot_git_dir.strip_prefix(state.snapshot.abs_path()) else {
                        // A `.git` path outside the worktree root is not
                        // ours to register. This happens legitimately when
                        // `.git` is a gitfile pointing outside the worktree
                        // (linked worktrees and submodules), and also when
                        // a rescan of a linked worktree's commondir arrives
                        // after the worktree's repository has already been
                        // unregistered.
                        continue;
                    };
                    affected_repo_roots.push(dot_git_dir.parent().unwrap().into());
                    state
                        .insert_git_repository(
                            RelPath::new(relative, PathStyle::local())
                                .unwrap()
                                .into_arc(),
                            self.fs.as_ref(),
                            self.watcher.as_ref(),
                        )
                        .await;
                }
                Some(local_repository) => {
                    state.snapshot.git_repositories.update(
                        &local_repository.work_directory_id,
                        |entry| {
                            entry.git_dir_scan_id = scan_id;
                        },
                    );
                }
            };
        }

        // Remove any git repositories whose .git entry no longer exists.
        let snapshot = &mut state.snapshot;
        let mut ids_to_preserve = HashSet::default();
        for (&work_directory_id, entry) in snapshot.git_repositories.iter() {
            let exists_in_snapshot =
                snapshot
                    .entry_for_id(work_directory_id)
                    .is_some_and(|entry| {
                        snapshot
                            .entry_for_path(&entry.path.join(RelPath::unix(DOT_GIT).unwrap()))
                            .is_some()
                    });

            // Only drop a repository when we can positively confirm that its git
            // directory is gone. `metadata` returns `Ok(None)` for a confirmed
            // absence, but `Err(_)` for a transient failure (which can happen
            // under heavy filesystem churn). Treating an error as a deletion
            // makes the repository flap out of and back into the snapshot,
            // causing the GitStore to repeatedly tear it down and re-create it
            // with a fresh `RepositoryId`. So preserve the repository unless the
            // `.git` entry is confirmed absent.
            let dot_git_present =
                !matches!(self.fs.metadata(&entry.dot_git_abs_path).await, Ok(None));

            if exists_in_snapshot || dot_git_present {
                ids_to_preserve.insert(work_directory_id);
            }
        }

        snapshot
            .git_repositories
            .retain(|work_directory_id, entry| {
                let preserve = ids_to_preserve.contains(work_directory_id);
                if !preserve {
                    affected_repo_roots.push(entry.dot_git_abs_path.parent().unwrap().into());
                    snapshot
                        .repo_exclude_by_work_dir_abs_path
                        .remove(&entry.work_directory_abs_path);
                }
                preserve
            });

        affected_repo_roots
    }
}

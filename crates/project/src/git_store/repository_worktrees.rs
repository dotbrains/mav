use super::*;

impl Repository {
    pub fn worktrees(&mut self) -> oneshot::Receiver<Result<Vec<GitWorktree>>> {
        let id = self.id;
        self.send_job("worktrees", None, move |repo, _| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.worktrees().await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitGetWorktrees {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        })
                        .await?;

                    let worktrees = response
                        .worktrees
                        .into_iter()
                        .map(|worktree| proto_to_worktree(&worktree))
                        .collect();

                    Ok(worktrees)
                }
            }
        })
    }

    pub fn create_worktree(
        &mut self,
        target: CreateWorktreeTarget,
        path: PathBuf,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        let job_description = match target.branch_name() {
            Some(branch_name) => format!("git worktree add: {branch_name}"),
            None => "git worktree add (detached)".to_string(),
        };
        self.send_job(
            "create_worktree",
            Some(job_description.into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.create_worktree(target, path).await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        let (name, commit, use_existing_branch) = match target {
                            CreateWorktreeTarget::ExistingBranch { branch_name } => {
                                (Some(branch_name), None, true)
                            }
                            CreateWorktreeTarget::NewBranch {
                                branch_name,
                                base_sha,
                            } => (Some(branch_name), base_sha, false),
                            CreateWorktreeTarget::Detached { base_sha } => (None, base_sha, false),
                        };

                        client
                            .request(proto::GitCreateWorktree {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                name: name.unwrap_or_default(),
                                directory: path.to_string_lossy().to_string(),
                                commit,
                                use_existing_branch,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    /// Returns the creation time of a linked worktree's git metadata
    /// directory. See [`GitRepository::worktree_created_at`]. For remote
    /// projects the stat runs on the remote host, where the worktree's
    /// filesystem lives.
    pub fn worktree_created_at(
        &mut self,
        worktree_path: PathBuf,
    ) -> oneshot::Receiver<Result<Option<SystemTime>>> {
        let id = self.id;
        self.send_job("worktree_created_at", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.worktree_created_at(worktree_path).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitWorktreeCreatedAt {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            worktree_path: worktree_path.to_string_lossy().to_string(),
                        })
                        .await?;
                    Ok(response.created_at.map(SystemTime::from))
                }
            }
        })
    }

    pub fn create_worktree_detached(
        &mut self,
        path: PathBuf,
        commit: String,
    ) -> oneshot::Receiver<Result<()>> {
        self.create_worktree(
            CreateWorktreeTarget::Detached {
                base_sha: Some(commit),
            },
            path,
        )
    }

    pub fn checkout_branch_in_worktree(
        &mut self,
        branch_name: String,
        worktree_path: PathBuf,
        create: bool,
    ) -> oneshot::Receiver<Result<()>> {
        let description = if create {
            format!("git checkout -b {branch_name}")
        } else {
            format!("git checkout {branch_name}")
        };
        self.send_job(
            "checkout_branch_in_worktree",
            Some(description.into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend
                            .checkout_branch_in_worktree(branch_name, worktree_path, create)
                            .await
                    }
                    RepositoryState::Remote(_) => {
                        log::warn!(
                            "checkout_branch_in_worktree not supported for remote repositories"
                        );
                        Ok(())
                    }
                }
            },
        )
    }

    pub fn head_sha(&mut self) -> oneshot::Receiver<Result<Option<String>>> {
        let id = self.id;
        self.send_job("head_sha", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    Ok(backend.head_sha().await)
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitGetHeadSha {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        })
                        .await?;

                    Ok(response.sha)
                }
            }
        })
    }

    pub(super) fn edit_ref(
        &mut self,
        ref_name: String,
        commit: Option<String>,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job("edit_ref", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => match commit {
                    Some(commit) => backend.update_ref(ref_name, commit).await,
                    None => backend.delete_ref(ref_name).await,
                },
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let action = match commit {
                        Some(sha) => proto::git_edit_ref::Action::UpdateToCommit(sha),
                        None => {
                            proto::git_edit_ref::Action::Delete(proto::git_edit_ref::DeleteRef {})
                        }
                    };
                    client
                        .request(proto::GitEditRef {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            ref_name,
                            action: Some(action),
                        })
                        .await?;
                    Ok(())
                }
            }
        })
    }

    pub fn update_ref(
        &mut self,
        ref_name: String,
        commit: String,
    ) -> oneshot::Receiver<Result<()>> {
        self.edit_ref(ref_name, Some(commit))
    }

    pub fn delete_ref(&mut self, ref_name: String) -> oneshot::Receiver<Result<()>> {
        self.edit_ref(ref_name, None)
    }

    pub fn repair_worktrees(&mut self) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job("repair_worktrees", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.repair_worktrees().await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    client
                        .request(proto::GitRepairWorktrees {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        })
                        .await?;
                    Ok(())
                }
            }
        })
    }

    pub fn create_archive_checkpoint(&mut self) -> oneshot::Receiver<Result<(String, String)>> {
        let id = self.id;
        self.send_job(
            "create_archive_checkpoint",
            None,
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.create_archive_checkpoint().await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        let response = client
                            .request(proto::GitCreateArchiveCheckpoint {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                            })
                            .await?;
                        Ok((response.staged_commit_sha, response.unstaged_commit_sha))
                    }
                }
            },
        )
    }

    pub fn restore_archive_checkpoint(
        &mut self,
        staged_sha: String,
        unstaged_sha: String,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job(
            "restore_archive_checkpoint",
            None,
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend
                            .restore_archive_checkpoint(staged_sha, unstaged_sha)
                            .await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitRestoreArchiveCheckpoint {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                staged_commit_sha: staged_sha,
                                unstaged_commit_sha: unstaged_sha,
                            })
                            .await?;
                        Ok(())
                    }
                }
            },
        )
    }

    pub fn remove_worktree(&mut self, path: PathBuf, force: bool) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        let repository_anchor_path: Arc<Path> = self
            .snapshot
            .main_worktree_abs_path()
            .unwrap_or(self.snapshot.common_dir_abs_path.as_ref())
            .into();
        self.send_job(
            "remove_worktree",
            Some(format!("git worktree remove: {}", path.display()).into()),
            move |repo, cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, fs, .. }) => {
                        // When forcing, delete the worktree directory ourselves before
                        // invoking git. `git worktree remove` can remove the admin
                        // metadata in `.git/worktrees/<name>` but fail to delete the
                        // working directory (it continues past directory-removal errors),
                        // leaving an orphaned folder on disk. Deleting first guarantees
                        // the directory is gone, and `git worktree remove --force`
                        // tolerates a missing working tree while cleaning up the admin
                        // entry. We keep this inside the `Local` arm so that for remote
                        // projects the deletion runs on the remote machine (where the
                        // `GitRemoveWorktree` RPC is handled against the local repo on
                        // the headless server) using its own filesystem.
                        //
                        // After a successful removal, also delete any empty ancestor
                        // directories between the worktree path and the configured
                        // base directory used when creating linked worktrees.
                        //
                        // Non-force removals are left untouched before git runs:
                        // `git worktree remove` must see the dirty working tree to
                        // refuse the operation.
                        if force {
                            fs.remove_dir(
                                &path,
                                RemoveOptions {
                                    recursive: true,
                                    ignore_if_not_exists: true,
                                },
                            )
                            .await
                            .with_context(|| {
                                format!("failed to delete worktree directory '{}'", path.display())
                            })?;
                        }

                        backend.remove_worktree(path.clone(), force).await?;

                        let managed_worktree_base = cx.update(|cx| {
                            let setting = &ProjectSettings::get_global(cx).git.worktree_directory;
                            worktrees_directory_for_repo(
                                &repository_anchor_path,
                                setting,
                                PathStyle::local(),
                            )
                            .log_err()
                        });

                        if let Some(managed_worktree_base) = managed_worktree_base {
                            remove_empty_managed_worktree_ancestors(
                                fs.as_ref(),
                                &path,
                                &managed_worktree_base,
                            )
                            .await;
                        }

                        Ok(())
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitRemoveWorktree {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                path: path.to_string_lossy().to_string(),
                                force,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn rename_worktree(
        &mut self,
        old_path: PathBuf,
        new_path: PathBuf,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job(
            "rename_worktree",
            Some(format!("git worktree move: {}", old_path.display()).into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.rename_worktree(old_path, new_path).await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitRenameWorktree {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                old_path: old_path.to_string_lossy().to_string(),
                                new_path: new_path.to_string_lossy().to_string(),
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }
}

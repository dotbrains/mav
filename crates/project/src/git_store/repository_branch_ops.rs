use super::*;

impl Repository {
    pub fn create_remote(
        &mut self,
        remote_name: String,
        remote_url: String,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job(
            "create_remote",
            Some(format!("git remote add {remote_name} {remote_url}").into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.create_remote(remote_name, remote_url).await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitCreateRemote {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                remote_name,
                                remote_url,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn remove_remote(&mut self, remote_name: String) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job(
            "remove_remote",
            Some(format!("git remove remote {remote_name}").into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.remove_remote(remote_name).await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitRemoveRemote {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                remote_name,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn get_remotes(
        &mut self,
        branch_name: Option<String>,
        is_push: bool,
    ) -> oneshot::Receiver<Result<Vec<Remote>>> {
        let id = self.id;
        self.send_job("get_remotes", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    let remote = if let Some(branch_name) = branch_name {
                        if is_push {
                            backend.get_push_remote(branch_name).await?
                        } else {
                            backend.get_branch_remote(branch_name).await?
                        }
                    } else {
                        None
                    };

                    match remote {
                        Some(remote) => Ok(vec![remote]),
                        None => backend.get_all_remotes().await,
                    }
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GetRemotes {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            branch_name,
                            is_push,
                        })
                        .await?;

                    let remotes = response
                        .remotes
                        .into_iter()
                        .map(|remotes| Remote {
                            name: remotes.name.into(),
                        })
                        .collect();

                    Ok(remotes)
                }
            }
        })
    }

    pub fn branches(&mut self) -> oneshot::Receiver<Result<BranchesScanResult>> {
        let id = self.id;
        self.send_job("branches", None, move |repo, _| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.branches().await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitGetBranches {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        })
                        .await?;

                    let branches = response
                        .branches
                        .into_iter()
                        .map(|branch| proto_to_branch(&branch))
                        .collect();

                    Ok(BranchesScanResult {
                        branches,
                        error: response.error.map(SharedString::from),
                    })
                }
            }
        })
    }

    /// If this is a linked worktree (*NOT* the main checkout of a repository),
    /// returns the path for the linked worktree.
    ///
    /// Returns None if this is the main checkout.
    pub fn linked_worktree_path(&self) -> Option<&Arc<Path>> {
        self.snapshot
            .is_linked_worktree()
            .then_some(&self.work_directory_abs_path)
    }

    pub fn path_for_new_linked_worktree(
        &self,
        branch_name: &str,
        worktree_directory_setting: &str,
    ) -> Result<PathBuf> {
        let repository_anchor = self
            .snapshot
            .main_worktree_abs_path()
            .unwrap_or(self.common_dir_abs_path.as_ref());
        let project_name = repository_anchor
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("git repo must have a directory name"))?;
        let directory = worktrees_directory_for_repo(
            repository_anchor,
            worktree_directory_setting,
            self.path_style,
        )?;
        let directory = self.path_style.join_path(&directory, branch_name)?;
        self.path_style.join_path(&directory, project_name)
    }

    pub fn default_branch(
        &mut self,
        include_remote_name: bool,
    ) -> oneshot::Receiver<Result<Option<SharedString>>> {
        let id = self.id;
        self.send_job("default_branch", None, move |repo, _| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.default_branch(include_remote_name).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GetDefaultBranch {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        })
                        .await?;

                    anyhow::Ok(response.branch.map(SharedString::from))
                }
            }
        })
    }

    pub fn diff_tree(
        &mut self,
        diff_type: DiffTreeType,
        _cx: &App,
    ) -> oneshot::Receiver<Result<TreeDiff>> {
        let repository_id = self.snapshot.id;
        self.send_job("diff_tree", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.diff_tree(diff_type).await
                }
                RepositoryState::Remote(RemoteRepositoryState { client, project_id }) => {
                    let response = client
                        .request(proto::GetTreeDiff {
                            project_id: project_id.0,
                            repository_id: repository_id.0,
                            is_merge: matches!(diff_type, DiffTreeType::MergeBase { .. }),
                            base: diff_type.base().to_string(),
                            head: diff_type.head().to_string(),
                        })
                        .await?;

                    let entries = response
                        .entries
                        .into_iter()
                        .filter_map(|entry| {
                            let status = match entry.status() {
                                proto::tree_diff_status::Status::Added => TreeDiffStatus::Added,
                                proto::tree_diff_status::Status::Modified => {
                                    TreeDiffStatus::Modified {
                                        old: git::Oid::from_str(
                                            &entry.oid.context("missing oid").log_err()?,
                                        )
                                        .log_err()?,
                                    }
                                }
                                proto::tree_diff_status::Status::Deleted => {
                                    TreeDiffStatus::Deleted {
                                        old: git::Oid::from_str(
                                            &entry.oid.context("missing oid").log_err()?,
                                        )
                                        .log_err()?,
                                    }
                                }
                            };
                            Some((
                                RepoPath::from_rel_path(
                                    &RelPath::from_proto(&entry.path).log_err()?,
                                ),
                                status,
                            ))
                        })
                        .collect();

                    Ok(TreeDiff { entries })
                }
            }
        })
    }

    pub fn diff(&mut self, diff_type: DiffType, _cx: &App) -> oneshot::Receiver<Result<String>> {
        let id = self.id;
        self.send_job("diff", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.diff(diff_type).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let (proto_diff_type, merge_base_ref) = match &diff_type {
                        DiffType::HeadToIndex => {
                            (proto::git_diff::DiffType::HeadToIndex.into(), None)
                        }
                        DiffType::HeadToWorktree => {
                            (proto::git_diff::DiffType::HeadToWorktree.into(), None)
                        }
                        DiffType::MergeBase { base_ref } => (
                            proto::git_diff::DiffType::MergeBase.into(),
                            Some(base_ref.to_string()),
                        ),
                    };
                    let response = client
                        .request(proto::GitDiff {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            diff_type: proto_diff_type,
                            merge_base_ref,
                        })
                        .await?;

                    Ok(response.diff)
                }
            }
        })
    }

    pub fn create_branch(
        &mut self,
        branch_name: String,
        base_branch: Option<String>,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        let status_msg = if let Some(ref base) = base_branch {
            format!("git switch -c {branch_name} {base}").into()
        } else {
            format!("git switch -c {branch_name}").into()
        };
        self.send_job(
            "create_branch",
            Some(status_msg),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.create_branch(branch_name, base_branch).await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitCreateBranch {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                branch_name,
                                base_branch,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn change_branch(&mut self, branch_name: String) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job(
            "change_branch",
            Some(format!("git switch {branch_name}").into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.change_branch(branch_name).await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitChangeBranch {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                branch_name,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn delete_branch(
        &mut self,
        is_remote: bool,
        branch_name: String,
        force: bool,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        let flag = delete_branch_flag(is_remote, force);
        self.send_job(
            "delete_branch",
            Some(format!("git branch {flag} {branch_name}").into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(state) => {
                        state
                            .backend
                            .delete_branch(is_remote, branch_name, force)
                            .await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitDeleteBranch {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                is_remote,
                                branch_name,
                                force,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn rename_branch(
        &mut self,
        branch: String,
        new_name: String,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job(
            "rename_branch",
            Some(format!("git branch -m {branch} {new_name}").into()),
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.rename_branch(branch, new_name).await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::GitRenameBranch {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                branch,
                                new_name,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn check_for_pushed_commits(&mut self) -> oneshot::Receiver<Result<Vec<SharedString>>> {
        let id = self.id;
        self.send_job(
            "check_for_pushed_commits",
            None,
            move |repo, _cx| async move {
                match repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.check_for_pushed_commit().await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        let response = client
                            .request(proto::CheckForPushedCommits {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                            })
                            .await?;

                        let branches = response.pushed_to.into_iter().map(Into::into).collect();

                        Ok(branches)
                    }
                }
            },
        )
    }
}

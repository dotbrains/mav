use super::*;

impl Repository {
    pub fn run_hook(&mut self, hook: RunHook, _cx: &mut App) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job(
            "run_hook",
            Some(format!("git hook {}", hook.as_str()).into()),
            move |git_repo, _cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState {
                        backend,
                        environment,
                        ..
                    }) => backend.run_hook(hook, environment.clone()).await,
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::RunGitHook {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                hook: hook.to_proto(),
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn commit(
        &mut self,
        message: SharedString,
        name_and_email: Option<(SharedString, SharedString)>,
        options: CommitOptions,
        askpass: AskPassDelegate,
        cx: &mut App,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        let askpass_delegates = self.askpass_delegates.clone();
        let askpass_id = util::post_inc(&mut self.latest_askpass_id);

        let rx = self.run_hook(RunHook::PreCommit, cx);

        self.send_job(
            "commit",
            Some("git commit".into()),
            move |git_repo, _cx| async move {
                rx.await??;

                match git_repo {
                    RepositoryState::Local(LocalRepositoryState {
                        backend,
                        environment,
                        ..
                    }) => {
                        backend
                            .commit(message, name_and_email, options, askpass, environment)
                            .await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        askpass_delegates.lock().insert(askpass_id, askpass);
                        let _defer = util::defer(|| {
                            let askpass_delegate = askpass_delegates.lock().remove(&askpass_id);
                            debug_assert!(askpass_delegate.is_some());
                        });
                        let (name, email) = name_and_email.unzip();
                        client
                            .request(proto::Commit {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                message: String::from(message),
                                name: name.map(String::from),
                                email: email.map(String::from),
                                options: Some(proto::commit::CommitOptions {
                                    amend: options.amend,
                                    signoff: options.signoff,
                                    allow_empty: options.allow_empty,
                                }),
                                askpass_id,
                            })
                            .await?;

                        Ok(())
                    }
                }
            },
        )
    }

    pub fn fetch(
        &mut self,
        fetch_options: FetchOptions,
        askpass: AskPassDelegate,
        _cx: &mut App,
    ) -> oneshot::Receiver<Result<RemoteCommandOutput>> {
        let askpass_delegates = self.askpass_delegates.clone();
        let askpass_id = util::post_inc(&mut self.latest_askpass_id);
        let id = self.id;

        self.send_job(
            "fetch",
            Some("git fetch".into()),
            move |git_repo, cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState {
                        backend,
                        environment,
                        ..
                    }) => backend.fetch(fetch_options, askpass, environment, cx).await,
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        askpass_delegates.lock().insert(askpass_id, askpass);
                        let _defer = util::defer(|| {
                            let askpass_delegate = askpass_delegates.lock().remove(&askpass_id);
                            debug_assert!(askpass_delegate.is_some());
                        });

                        let response = client
                            .request(proto::Fetch {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                askpass_id,
                                remote: fetch_options.to_proto(),
                            })
                            .await?;

                        Ok(RemoteCommandOutput {
                            stdout: response.stdout,
                            stderr: response.stderr,
                        })
                    }
                }
            },
        )
    }

    pub fn push(
        &mut self,
        branch: SharedString,
        remote_branch: SharedString,
        remote: SharedString,
        options: Option<PushOptions>,
        askpass: AskPassDelegate,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Result<RemoteCommandOutput>> {
        let askpass_delegates = self.askpass_delegates.clone();
        let askpass_id = util::post_inc(&mut self.latest_askpass_id);
        let id = self.id;

        let args = options
            .map(|option| match option {
                PushOptions::SetUpstream => " --set-upstream",
                PushOptions::Force => " --force-with-lease",
            })
            .unwrap_or("");

        let updates_tx = self
            .git_store()
            .and_then(|git_store| match &git_store.read(cx).state {
                GitStoreState::Local { downstream, .. } => downstream
                    .as_ref()
                    .map(|downstream| downstream.updates_tx.clone()),
                _ => None,
            });

        let this = cx.weak_entity();
        self.send_job(
            "push",
            Some(format!("git push {} {} {}:{}", args, remote, branch, remote_branch).into()),
            move |git_repo, mut cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState {
                        backend,
                        environment,
                        ..
                    }) => {
                        let result = backend
                            .push(
                                branch.to_string(),
                                remote_branch.to_string(),
                                remote.to_string(),
                                options,
                                askpass,
                                environment.clone(),
                                cx.clone(),
                            )
                            .await;
                        // TODO would be nice to not have to do this manually
                        if result.is_ok() {
                            let branches_scan = backend.branches().await?;
                            let branch_list_error = branches_scan.error;
                            let branch_list: Arc<[Branch]> = branches_scan.branches.into();
                            let branch = branch_list.iter().find(|branch| branch.is_head).cloned();
                            log::info!("head branch after scan is {branch:?}");
                            let snapshot = this.update(&mut cx, |this, cx| {
                                let branch_list_changed =
                                    *branch_list != *this.snapshot.branch_list;
                                let branch_list_error_changed =
                                    this.snapshot.branch_list_error != branch_list_error;
                                this.snapshot.branch = branch;
                                this.snapshot.branch_list = branch_list;
                                this.snapshot.branch_list_error = branch_list_error;
                                cx.emit(RepositoryEvent::HeadChanged);
                                if branch_list_changed || branch_list_error_changed {
                                    cx.emit(RepositoryEvent::BranchListChanged);
                                }
                                this.snapshot.clone()
                            })?;
                            if let Some(updates_tx) = updates_tx {
                                updates_tx
                                    .unbounded_send(DownstreamUpdate::UpdateRepository(snapshot))
                                    .ok();
                            }
                        }
                        result
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        askpass_delegates.lock().insert(askpass_id, askpass);
                        let _defer = util::defer(|| {
                            let askpass_delegate = askpass_delegates.lock().remove(&askpass_id);
                            debug_assert!(askpass_delegate.is_some());
                        });
                        let response = client
                            .request(proto::Push {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                askpass_id,
                                branch_name: branch.to_string(),
                                remote_branch_name: remote_branch.to_string(),
                                remote_name: remote.to_string(),
                                options: options.map(|options| match options {
                                    PushOptions::Force => proto::push::PushOptions::Force,
                                    PushOptions::SetUpstream => {
                                        proto::push::PushOptions::SetUpstream
                                    }
                                }
                                    as i32),
                            })
                            .await?;

                        Ok(RemoteCommandOutput {
                            stdout: response.stdout,
                            stderr: response.stderr,
                        })
                    }
                }
            },
        )
    }

    pub fn pull(
        &mut self,
        branch: Option<SharedString>,
        remote: SharedString,
        rebase: bool,
        askpass: AskPassDelegate,
        _cx: &mut App,
    ) -> oneshot::Receiver<Result<RemoteCommandOutput>> {
        let askpass_delegates = self.askpass_delegates.clone();
        let askpass_id = util::post_inc(&mut self.latest_askpass_id);
        let id = self.id;

        let mut status = "git pull".to_string();
        if rebase {
            status.push_str(" --rebase");
        }
        status.push_str(&format!(" {}", remote));
        if let Some(b) = &branch {
            status.push_str(&format!(" {}", b));
        }

        self.send_job(
            "pull",
            Some(status.into()),
            move |git_repo, cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState {
                        backend,
                        environment,
                        ..
                    }) => {
                        backend
                            .pull(
                                branch.as_ref().map(|b| b.to_string()),
                                remote.to_string(),
                                rebase,
                                askpass,
                                environment.clone(),
                                cx,
                            )
                            .await
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        askpass_delegates.lock().insert(askpass_id, askpass);
                        let _defer = util::defer(|| {
                            let askpass_delegate = askpass_delegates.lock().remove(&askpass_id);
                            debug_assert!(askpass_delegate.is_some());
                        });
                        let response = client
                            .request(proto::Pull {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                askpass_id,
                                rebase,
                                branch_name: branch.as_ref().map(|b| b.to_string()),
                                remote_name: remote.to_string(),
                            })
                            .await?;

                        Ok(RemoteCommandOutput {
                            stdout: response.stdout,
                            stderr: response.stderr,
                        })
                    }
                }
            },
        )
    }
}

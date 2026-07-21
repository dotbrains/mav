use super::*;

impl Repository {
    pub(super) fn load_staged_text(
        &mut self,
        buffer_id: BufferId,
        repo_path: RepoPath,
        cx: &App,
    ) -> Task<Result<Option<String>>> {
        let rx = self.send_job("load_staged_text", None, move |state, _| async move {
            match state {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    anyhow::Ok(backend.load_index_text(repo_path).await)
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::OpenUnstagedDiff {
                            project_id: project_id.to_proto(),
                            buffer_id: buffer_id.to_proto(),
                        })
                        .await?;
                    Ok(response.staged_text)
                }
            }
        });
        cx.spawn(|_: &mut AsyncApp| async move { rx.await? })
    }

    pub(super) fn load_committed_text(
        &mut self,
        buffer_id: BufferId,
        repo_path: RepoPath,
        cx: &App,
    ) -> Task<Result<DiffBasesChange>> {
        let rx = self.send_job("load_committed_text", None, move |state, _| async move {
            match state {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    let committed_text = backend.load_committed_text(repo_path.clone()).await;
                    let staged_text = backend.load_index_text(repo_path).await;
                    let diff_bases_change = if committed_text == staged_text {
                        DiffBasesChange::SetBoth(committed_text)
                    } else {
                        DiffBasesChange::SetEach {
                            index: staged_text,
                            head: committed_text,
                        }
                    };
                    anyhow::Ok(diff_bases_change)
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    use proto::open_uncommitted_diff_response::Mode;

                    let response = client
                        .request(proto::OpenUncommittedDiff {
                            project_id: project_id.to_proto(),
                            buffer_id: buffer_id.to_proto(),
                        })
                        .await?;
                    let mode = Mode::from_i32(response.mode).context("Invalid mode")?;
                    let bases = match mode {
                        Mode::IndexMatchesHead => DiffBasesChange::SetBoth(response.committed_text),
                        Mode::IndexAndHead => DiffBasesChange::SetEach {
                            head: response.committed_text,
                            index: response.staged_text,
                        },
                    };
                    Ok(bases)
                }
            }
        });

        cx.spawn(|_: &mut AsyncApp| async move { rx.await? })
    }

    pub fn load_commit_template_text(
        &mut self,
    ) -> oneshot::Receiver<Result<Option<GitCommitTemplate>>> {
        self.send_job(
            "load_commit_template_text",
            None,
            move |git_repo, _cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.load_commit_template().await
                    }
                    RepositoryState::Remote(_) => Ok(None),
                }
            },
        )
    }

    pub(super) fn load_blob_content(&mut self, oid: Oid, cx: &App) -> Task<Result<String>> {
        let repository_id = self.snapshot.id;
        let rx = self.send_job("load_blob_content", None, move |state, _| async move {
            match state {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.load_blob_content(oid).await
                }
                RepositoryState::Remote(RemoteRepositoryState { client, project_id }) => {
                    let response = client
                        .request(proto::GetBlobContent {
                            project_id: project_id.to_proto(),
                            repository_id: repository_id.0,
                            oid: oid.to_string(),
                        })
                        .await?;
                    Ok(response.content)
                }
            }
        });
        cx.spawn(|_: &mut AsyncApp| async move { rx.await? })
    }
}

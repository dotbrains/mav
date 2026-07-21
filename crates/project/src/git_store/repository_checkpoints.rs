use super::*;

impl Repository {
    pub fn checkpoint(&mut self) -> oneshot::Receiver<Result<GitRepositoryCheckpoint>> {
        let id = self.id;
        self.send_job("checkpoint", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.checkpoint().await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitCreateCheckpoint {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        })
                        .await?;

                    Ok(GitRepositoryCheckpoint {
                        commit_sha: Oid::from_bytes(&response.commit_sha)?,
                    })
                }
            }
        })
    }

    pub fn restore_checkpoint(
        &mut self,
        checkpoint: GitRepositoryCheckpoint,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job("restore_checkpoint", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.restore_checkpoint(checkpoint).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    client
                        .request(proto::GitRestoreCheckpoint {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            commit_sha: checkpoint.commit_sha.as_bytes().to_vec(),
                        })
                        .await?;
                    Ok(())
                }
            }
        })
    }

    pub fn compare_checkpoints(
        &mut self,
        left: GitRepositoryCheckpoint,
        right: GitRepositoryCheckpoint,
    ) -> oneshot::Receiver<Result<bool>> {
        let id = self.id;
        self.send_job("compare_checkpoints", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.compare_checkpoints(left, right).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitCompareCheckpoints {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            left_commit_sha: left.commit_sha.as_bytes().to_vec(),
                            right_commit_sha: right.commit_sha.as_bytes().to_vec(),
                        })
                        .await?;
                    Ok(response.equal)
                }
            }
        })
    }

    pub fn diff_checkpoints(
        &mut self,
        base_checkpoint: GitRepositoryCheckpoint,
        target_checkpoint: GitRepositoryCheckpoint,
    ) -> oneshot::Receiver<Result<String>> {
        let id = self.id;
        self.send_job("diff_checkpoints", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend
                        .diff_checkpoints(base_checkpoint, target_checkpoint)
                        .await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitDiffCheckpoints {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            base_commit_sha: base_checkpoint.commit_sha.as_bytes().to_vec(),
                            target_commit_sha: target_checkpoint.commit_sha.as_bytes().to_vec(),
                        })
                        .await?;
                    Ok(response.diff)
                }
            }
        })
    }
}

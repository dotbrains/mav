use super::*;

impl Repository {
    pub fn access(&mut self, _cx: &App) -> oneshot::Receiver<GitAccess> {
        self.send_job("access", None, move |git_repo, _cx| async move {
            match git_repo {
                // TODO: Correctly handle remote repositories, where the user
                // that's running the Mav remote may not own the `.git/`
                // directory. For now we just return `GitAccess::Yes` so that
                // remoting continues working as expected.
                RepositoryState::Remote(..) => GitAccess::Yes,
                RepositoryState::Local(state) => match state.backend.check_access().await {
                    Ok(_) => GitAccess::Yes,
                    Err(_) => GitAccess::No,
                },
            }
        })
    }

    pub fn default_remote_url(&self) -> Option<String> {
        self.remote_upstream_url
            .clone()
            .or(self.remote_origin_url.clone())
    }
}

use super::*;

impl GitStore {
    pub(super) async fn handle_set_index_text(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::SetIndexText>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let repo_path = RepoPath::from_proto(&envelope.payload.path)?;

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.spawn_set_index_text_job(
                    repo_path,
                    envelope.payload.text,
                    None,
                    cx,
                )
            })
            .await??;
        Ok(proto::Ack {})
    }

    pub(super) async fn handle_run_hook(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RunGitHook>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let hook = RunHook::from_proto(envelope.payload.hook).context("invalid hook")?;
        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.run_hook(hook, cx)
            })
            .await??;
        Ok(proto::Ack {})
    }

    pub(super) async fn handle_commit(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Commit>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let askpass_id = envelope.payload.askpass_id;

        let askpass = super::remote_delegate::make_remote_delegate(
            this,
            envelope.payload.project_id,
            repository_id,
            askpass_id,
            &mut cx,
        );

        let message = SharedString::from(envelope.payload.message);
        let name = envelope.payload.name.map(SharedString::from);
        let email = envelope.payload.email.map(SharedString::from);
        let options = envelope.payload.options.unwrap_or_default();

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.commit(
                    message,
                    name.zip(email),
                    CommitOptions {
                        amend: options.amend,
                        signoff: options.signoff,
                        allow_empty: options.allow_empty,
                    },
                    askpass,
                    cx,
                )
            })
            .await??;
        Ok(proto::Ack {})
    }

    pub(super) async fn handle_get_remotes(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetRemotes>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetRemotesResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let branch_name = envelope.payload.branch_name;
        let is_push = envelope.payload.is_push;

        let remotes = repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.get_remotes(branch_name, is_push)
            })
            .await??;

        Ok(proto::GetRemotesResponse {
            remotes: remotes
                .into_iter()
                .map(|remotes| proto::get_remotes_response::Remote {
                    name: remotes.name.to_string(),
                })
                .collect::<Vec<_>>(),
        })
    }
}

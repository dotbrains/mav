use super::*;

impl GitStore {
    pub(super) async fn handle_get_branches(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitGetBranches>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitBranchesResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let branches_scan = repository_handle
            .update(&mut cx, |repository_handle, _| repository_handle.branches())
            .await??;

        Ok(proto::GitBranchesResponse {
            branches: branches_scan
                .branches
                .into_iter()
                .map(|branch| branch_to_proto(&branch))
                .collect::<Vec<_>>(),
            error: branches_scan.error.map(|error| error.to_string()),
        })
    }
    pub(super) async fn handle_get_default_branch(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetDefaultBranch>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetDefaultBranchResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let branch = repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.default_branch(false)
            })
            .await??
            .map(Into::into);

        Ok(proto::GetDefaultBranchResponse { branch })
    }
    pub(super) async fn handle_create_branch(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitCreateBranch>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let branch_name = envelope.payload.branch_name;
        let base_branch = envelope.payload.base_branch;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.create_branch(branch_name, base_branch)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_change_branch(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitChangeBranch>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let branch_name = envelope.payload.branch_name;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.change_branch(branch_name)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_rename_branch(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRenameBranch>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let branch = envelope.payload.branch;
        let new_name = envelope.payload.new_name;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.rename_branch(branch, new_name)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_create_remote(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitCreateRemote>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let remote_name = envelope.payload.remote_name;
        let remote_url = envelope.payload.remote_url;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.create_remote(remote_name, remote_url)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_delete_branch(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitDeleteBranch>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let is_remote = envelope.payload.is_remote;
        let branch_name = envelope.payload.branch_name;
        let force = envelope.payload.force;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.delete_branch(is_remote, branch_name, force)
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_remove_remote(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRemoveRemote>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let remote_name = envelope.payload.remote_name;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.remove_remote(remote_name)
            })
            .await??;

        Ok(proto::Ack {})
    }
}

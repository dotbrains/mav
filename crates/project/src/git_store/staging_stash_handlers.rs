use super::*;

impl GitStore {
    pub(super) async fn handle_stage(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Stage>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let entries = envelope
            .payload
            .paths
            .into_iter()
            .map(|path| RepoPath::new(&path))
            .collect::<Result<Vec<_>>>()?;

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.stage_entries(entries, cx)
            })
            .await?;
        Ok(proto::Ack {})
    }

    pub(super) async fn handle_unstage(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Unstage>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let entries = envelope
            .payload
            .paths
            .into_iter()
            .map(|path| RepoPath::new(&path))
            .collect::<Result<Vec<_>>>()?;

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.unstage_entries(entries, cx)
            })
            .await?;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_stash(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Stash>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let entries = envelope
            .payload
            .paths
            .into_iter()
            .map(|path| RepoPath::new(&path))
            .collect::<Result<Vec<_>>>()?;

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.stash_entries(entries, cx)
            })
            .await?;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_stash_pop(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::StashPop>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let stash_index = envelope.payload.stash_index.map(|i| i as usize);

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.stash_pop(stash_index, cx)
            })
            .await?;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_stash_apply(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::StashApply>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let stash_index = envelope.payload.stash_index.map(|i| i as usize);

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.stash_apply(stash_index, cx)
            })
            .await?;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_stash_drop(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::StashDrop>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let stash_index = envelope.payload.stash_index.map(|i| i as usize);

        repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.stash_drop(stash_index, cx)
            })
            .await??;

        Ok(proto::Ack {})
    }
}

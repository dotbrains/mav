use super::*;

impl Worktree {
    pub async fn handle_create_entry(
        this: Entity<Self>,
        request: proto::CreateProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ProjectEntryResponse> {
        let (scan_id, entry) = this.update(&mut cx, |this, cx| {
            anyhow::Ok((
                this.scan_id(),
                this.create_entry(
                    RelPath::from_proto(&request.path).with_context(|| {
                        format!("received invalid relative path {:?}", request.path)
                    })?,
                    request.is_directory,
                    request.content,
                    cx,
                ),
            ))
        })?;
        Ok(proto::ProjectEntryResponse {
            entry: match &entry.await? {
                CreatedEntry::Included(entry) => Some(entry.into()),
                CreatedEntry::Excluded { .. } => None,
            },
            worktree_scan_id: scan_id as u64,
        })
    }

    pub async fn handle_delete_entry(
        this: Entity<Self>,
        request: proto::DeleteProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ProjectEntryResponse> {
        let (scan_id, task) = this.update(&mut cx, |this, cx| {
            (
                this.scan_id(),
                this.delete_entry(
                    ProjectEntryId::from_proto(request.entry_id),
                    request.use_trash,
                    cx,
                ),
            )
        });
        task.ok_or_else(|| anyhow::anyhow!("invalid entry"))?
            .await?;
        Ok(proto::ProjectEntryResponse {
            entry: None,
            worktree_scan_id: scan_id as u64,
        })
    }

    pub async fn handle_expand_entry(
        this: Entity<Self>,
        request: proto::ExpandProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ExpandProjectEntryResponse> {
        let task = this.update(&mut cx, |this, cx| {
            this.expand_entry(ProjectEntryId::from_proto(request.entry_id), cx)
        });
        task.ok_or_else(|| anyhow::anyhow!("no such entry"))?
            .await?;
        let scan_id = this.read_with(&cx, |this, _| this.scan_id());
        Ok(proto::ExpandProjectEntryResponse {
            worktree_scan_id: scan_id as u64,
        })
    }

    pub async fn handle_expand_all_for_entry(
        this: Entity<Self>,
        request: proto::ExpandAllForProjectEntry,
        mut cx: AsyncApp,
    ) -> Result<proto::ExpandAllForProjectEntryResponse> {
        let task = this.update(&mut cx, |this, cx| {
            this.expand_all_for_entry(ProjectEntryId::from_proto(request.entry_id), cx)
        });
        task.ok_or_else(|| anyhow::anyhow!("no such entry"))?
            .await?;
        let scan_id = this.read_with(&cx, |this, _| this.scan_id());
        Ok(proto::ExpandAllForProjectEntryResponse {
            worktree_scan_id: scan_id as u64,
        })
    }
}

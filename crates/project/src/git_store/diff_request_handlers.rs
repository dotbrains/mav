use super::*;

impl GitStore {
    pub(super) async fn handle_git_diff(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitDiff>,
        mut cx: AsyncApp,
    ) -> Result<proto::GitDiffResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let diff_type = match envelope.payload.diff_type() {
            proto::git_diff::DiffType::HeadToIndex => DiffType::HeadToIndex,
            proto::git_diff::DiffType::HeadToWorktree => DiffType::HeadToWorktree,
            proto::git_diff::DiffType::MergeBase => {
                let base_ref = envelope
                    .payload
                    .merge_base_ref
                    .ok_or_else(|| anyhow!("merge_base_ref is required for MergeBase diff type"))?;
                DiffType::MergeBase {
                    base_ref: base_ref.into(),
                }
            }
        };

        let mut diff = repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.diff(diff_type, cx)
            })
            .await??;
        const ONE_MB: usize = 1_000_000;
        if diff.len() > ONE_MB {
            diff = diff.chars().take(ONE_MB).collect()
        }

        Ok(proto::GitDiffResponse { diff })
    }

    pub(super) async fn handle_tree_diff(
        this: Entity<Self>,
        request: TypedEnvelope<proto::GetTreeDiff>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetTreeDiffResponse> {
        let repository_id = RepositoryId(request.payload.repository_id);
        let diff_type = if request.payload.is_merge {
            DiffTreeType::MergeBase {
                base: request.payload.base.into(),
                head: request.payload.head.into(),
            }
        } else {
            DiffTreeType::Since {
                base: request.payload.base.into(),
                head: request.payload.head.into(),
            }
        };

        let diff = this
            .update(&mut cx, |this, cx| {
                let repository = this.repositories().get(&repository_id)?;
                Some(repository.update(cx, |repo, cx| repo.diff_tree(diff_type, cx)))
            })
            .context("missing repository")?
            .await??;

        Ok(proto::GetTreeDiffResponse {
            entries: diff
                .entries
                .into_iter()
                .map(|(path, status)| proto::TreeDiffStatus {
                    path: path.as_ref().to_proto(),
                    status: match status {
                        TreeDiffStatus::Added {} => proto::tree_diff_status::Status::Added.into(),
                        TreeDiffStatus::Modified { .. } => {
                            proto::tree_diff_status::Status::Modified.into()
                        }
                        TreeDiffStatus::Deleted { .. } => {
                            proto::tree_diff_status::Status::Deleted.into()
                        }
                    },
                    oid: match status {
                        TreeDiffStatus::Deleted { old } | TreeDiffStatus::Modified { old } => {
                            Some(old.to_string())
                        }
                        TreeDiffStatus::Added => None,
                    },
                })
                .collect(),
        })
    }

    pub(super) async fn handle_get_blob_content(
        this: Entity<Self>,
        request: TypedEnvelope<proto::GetBlobContent>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetBlobContentResponse> {
        let oid = git::Oid::from_str(&request.payload.oid)?;
        let repository_id = RepositoryId(request.payload.repository_id);
        let content = this
            .update(&mut cx, |this, cx| {
                let repository = this.repositories().get(&repository_id)?;
                Some(repository.update(cx, |repo, cx| repo.load_blob_content(oid, cx)))
            })
            .context("missing repository")?
            .await?;
        Ok(proto::GetBlobContentResponse { content })
    }

    pub(super) async fn handle_open_unstaged_diff(
        this: Entity<Self>,
        request: TypedEnvelope<proto::OpenUnstagedDiff>,
        mut cx: AsyncApp,
    ) -> Result<proto::OpenUnstagedDiffResponse> {
        let buffer_id = BufferId::new(request.payload.buffer_id)?;
        let diff = this
            .update(&mut cx, |this, cx| {
                let buffer = this.buffer_store.read(cx).get(buffer_id)?;
                Some(this.open_unstaged_diff(buffer, cx))
            })
            .context("missing buffer")?
            .await?;
        this.update(&mut cx, |this, _| {
            let shared_diffs = this
                .shared_diffs
                .entry(request.original_sender_id.unwrap_or(request.sender_id))
                .or_default();
            shared_diffs.entry(buffer_id).or_default().unstaged = Some(diff.clone());
        });
        let staged_text = diff.read_with(&cx, |diff, cx| diff.base_text_string(cx));
        Ok(proto::OpenUnstagedDiffResponse { staged_text })
    }

    pub(super) async fn handle_open_uncommitted_diff(
        this: Entity<Self>,
        request: TypedEnvelope<proto::OpenUncommittedDiff>,
        mut cx: AsyncApp,
    ) -> Result<proto::OpenUncommittedDiffResponse> {
        let buffer_id = BufferId::new(request.payload.buffer_id)?;
        let diff = this
            .update(&mut cx, |this, cx| {
                let buffer = this.buffer_store.read(cx).get(buffer_id)?;
                Some(this.open_uncommitted_diff(buffer, cx))
            })
            .context("missing buffer")?
            .await?;
        this.update(&mut cx, |this, _| {
            let shared_diffs = this
                .shared_diffs
                .entry(request.original_sender_id.unwrap_or(request.sender_id))
                .or_default();
            shared_diffs.entry(buffer_id).or_default().uncommitted = Some(diff.clone());
        });
        this.read_with(&cx, |this, cx| {
            use proto::open_uncommitted_diff_response::Mode;

            let diff_state = this.diffs.get(&buffer_id).context("missing diff state")?;
            let diff_state = diff_state.read(cx);
            let index_matches_head = diff_state.index_matches_head();
            let index_text = diff_state.index_text.clone();
            let head_text = diff_state.head_text.clone();

            let response = if index_matches_head {
                proto::OpenUncommittedDiffResponse {
                    committed_text: head_text.map(|head| head.to_string()),
                    staged_text: None,
                    mode: Mode::IndexMatchesHead.into(),
                }
            } else {
                proto::OpenUncommittedDiffResponse {
                    committed_text: head_text.map(|head| head.to_string()),
                    staged_text: index_text.map(|index| index.to_string()),
                    mode: Mode::IndexAndHead.into(),
                }
            };
            anyhow::Ok(response)
        })
    }

    pub(super) async fn handle_update_diff_bases(
        this: Entity<Self>,
        request: TypedEnvelope<proto::UpdateDiffBases>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let buffer_id = BufferId::new(request.payload.buffer_id)?;
        this.update(&mut cx, |this, cx| {
            if let Some(diff_state) = this.diffs.get_mut(&buffer_id)
                && let Some(buffer) = this.buffer_store.read(cx).get(buffer_id)
            {
                let buffer = buffer.read(cx).text_snapshot();
                diff_state.update(cx, |diff_state, cx| {
                    diff_state.handle_base_texts_updated(buffer, request.payload, cx);
                })
            }
        });
        Ok(())
    }

    pub(super) async fn handle_blame_buffer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::BlameBuffer>,
        mut cx: AsyncApp,
    ) -> Result<proto::BlameBufferResponse> {
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        let version = deserialize_version(&envelope.payload.version);
        let buffer = this.read_with(&cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(version.clone())
            })
            .await?;
        let blame = this
            .update(&mut cx, |this, cx| {
                this.blame_buffer(&buffer, Some(version), cx)
            })
            .await?;
        Ok(serialize_blame_buffer_response(blame))
    }

    pub(super) async fn handle_get_permalink_to_line(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetPermalinkToLine>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetPermalinkToLineResponse> {
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        // let version = deserialize_version(&envelope.payload.version);
        let selection = {
            let proto_selection = envelope
                .payload
                .selection
                .context("no selection to get permalink for defined")?;
            proto_selection.start as u32..proto_selection.end as u32
        };
        let buffer = this.read_with(&cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        let permalink = this
            .update(&mut cx, |this, cx| {
                this.get_permalink_to_line(&buffer, selection, cx)
            })
            .await?;
        Ok(proto::GetPermalinkToLineResponse {
            permalink: permalink.to_string(),
        })
    }

    pub(super) fn repository_for_request(
        this: &Entity<Self>,
        id: RepositoryId,
        cx: &mut AsyncApp,
    ) -> Result<Entity<Repository>> {
        this.read_with(cx, |this, _| {
            this.repositories
                .get(&id)
                .context("missing repository handle")
                .cloned()
        })
    }
}

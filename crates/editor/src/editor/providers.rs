use super::*;

pub trait CollaborationHub {
    fn collaborators<'a>(&self, cx: &'a App) -> &'a HashMap<PeerId, Collaborator>;
    fn user_participant_indices<'a>(&self, cx: &'a App) -> &'a HashMap<u64, ParticipantIndex>;
    fn user_names(&self, cx: &App) -> HashMap<u64, SharedString>;
}

impl CollaborationHub for Entity<Project> {
    fn collaborators<'a>(&self, cx: &'a App) -> &'a HashMap<PeerId, Collaborator> {
        self.read(cx).collaborators()
    }

    fn user_participant_indices<'a>(&self, cx: &'a App) -> &'a HashMap<u64, ParticipantIndex> {
        self.read(cx).user_store().read(cx).participant_indices()
    }

    fn user_names(&self, cx: &App) -> HashMap<u64, SharedString> {
        let this = self.read(cx);
        let user_ids = this.collaborators().values().map(|c| c.user_id);
        this.user_store().read(cx).participant_names(user_ids, cx)
    }
}

pub trait SemanticsProvider {
    fn hover(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Option<Task<Option<Vec<project::Hover>>>>;

    fn inline_values(
        &self,
        buffer_handle: Entity<Buffer>,
        range: Range<text::Anchor>,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Vec<InlayHint>>>>;

    fn applicable_inlay_chunks(
        &self,
        buffer: &Entity<Buffer>,
        ranges: &[Range<text::Anchor>],
        cx: &mut App,
    ) -> Vec<Range<BufferRow>>;

    fn invalidate_inlay_hints(&self, for_buffers: &HashSet<BufferId>, cx: &mut App);

    fn inlay_hints(
        &self,
        invalidate: InvalidationStrategy,
        buffer: Entity<Buffer>,
        ranges: Vec<Range<text::Anchor>>,
        known_chunks: Option<(clock::Global, HashSet<Range<BufferRow>>)>,
        cx: &mut App,
    ) -> Option<HashMap<Range<BufferRow>, Task<Result<CacheInlayHints>>>>;

    fn semantic_tokens(
        &self,
        buffer: Entity<Buffer>,
        refresh: Option<RefreshForServer>,
        cx: &mut App,
    ) -> Option<Shared<Task<std::result::Result<BufferSemanticTokens, Arc<anyhow::Error>>>>>;

    fn supports_inlay_hints(&self, buffer: &Entity<Buffer>, cx: &mut App) -> bool;

    fn supports_semantic_tokens(&self, buffer: &Entity<Buffer>, cx: &mut App) -> bool;

    fn document_highlights(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Option<Task<Result<Vec<DocumentHighlight>>>>;

    fn definitions(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        kind: GotoDefinitionKind,
        cx: &mut App,
    ) -> Option<Task<Result<Option<Vec<LocationLink>>>>>;

    fn range_for_rename(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Task<Result<Option<Range<text::Anchor>>>>;

    fn perform_rename(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        new_name: String,
        cx: &mut App,
    ) -> Option<Task<Result<ProjectTransaction>>>;
}

impl SemanticsProvider for WeakEntity<Project> {
    fn hover(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Option<Task<Option<Vec<project::Hover>>>> {
        self.update(cx, |project, cx| project.hover(buffer, position, cx))
            .ok()
    }

    fn document_highlights(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Option<Task<Result<Vec<DocumentHighlight>>>> {
        self.update(cx, |project, cx| {
            project.document_highlights(buffer, position, cx)
        })
        .ok()
    }

    fn definitions(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        kind: GotoDefinitionKind,
        cx: &mut App,
    ) -> Option<Task<Result<Option<Vec<LocationLink>>>>> {
        self.update(cx, |project, cx| match kind {
            GotoDefinitionKind::Symbol => project.definitions(buffer, position, cx),
            GotoDefinitionKind::Declaration => project.declarations(buffer, position, cx),
            GotoDefinitionKind::Type => project.type_definitions(buffer, position, cx),
            GotoDefinitionKind::Implementation => project.implementations(buffer, position, cx),
        })
        .ok()
    }

    fn supports_inlay_hints(&self, buffer: &Entity<Buffer>, cx: &mut App) -> bool {
        self.update(cx, |project, cx| {
            if project
                .active_debug_session(cx)
                .is_some_and(|(session, _)| session.read(cx).any_stopped_thread())
            {
                return true;
            }

            buffer.update(cx, |buffer, cx| {
                project.any_language_server_supports_inlay_hints(buffer, cx)
            })
        })
        .unwrap_or(false)
    }

    fn supports_semantic_tokens(&self, buffer: &Entity<Buffer>, cx: &mut App) -> bool {
        self.update(cx, |project, cx| {
            buffer.update(cx, |buffer, cx| {
                project.any_language_server_supports_semantic_tokens(buffer, cx)
            })
        })
        .unwrap_or(false)
    }

    fn inline_values(
        &self,
        buffer_handle: Entity<Buffer>,
        range: Range<text::Anchor>,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Vec<InlayHint>>>> {
        self.update(cx, |project, cx| {
            let (session, active_stack_frame) = project.active_debug_session(cx)?;

            Some(project.inline_values(session, active_stack_frame, buffer_handle, range, cx))
        })
        .ok()
        .flatten()
    }

    fn applicable_inlay_chunks(
        &self,
        buffer: &Entity<Buffer>,
        ranges: &[Range<text::Anchor>],
        cx: &mut App,
    ) -> Vec<Range<BufferRow>> {
        self.update(cx, |project, cx| {
            project.lsp_store().update(cx, |lsp_store, cx| {
                lsp_store.applicable_inlay_chunks(buffer, ranges, cx)
            })
        })
        .unwrap_or_default()
    }

    fn invalidate_inlay_hints(&self, for_buffers: &HashSet<BufferId>, cx: &mut App) {
        self.update(cx, |project, cx| {
            project.lsp_store().update(cx, |lsp_store, _| {
                lsp_store.invalidate_inlay_hints(for_buffers)
            })
        })
        .ok();
    }

    fn inlay_hints(
        &self,
        invalidate: InvalidationStrategy,
        buffer: Entity<Buffer>,
        ranges: Vec<Range<text::Anchor>>,
        known_chunks: Option<(clock::Global, HashSet<Range<BufferRow>>)>,
        cx: &mut App,
    ) -> Option<HashMap<Range<BufferRow>, Task<Result<CacheInlayHints>>>> {
        self.update(cx, |project, cx| {
            project.lsp_store().update(cx, |lsp_store, cx| {
                lsp_store.inlay_hints(invalidate, buffer, ranges, known_chunks, cx)
            })
        })
        .ok()
    }

    fn semantic_tokens(
        &self,
        buffer: Entity<Buffer>,
        refresh: Option<RefreshForServer>,
        cx: &mut App,
    ) -> Option<Shared<Task<std::result::Result<BufferSemanticTokens, Arc<anyhow::Error>>>>> {
        self.update(cx, |this, cx| {
            this.lsp_store().update(cx, |lsp_store, cx| {
                lsp_store.semantic_tokens(buffer, refresh, cx)
            })
        })
        .ok()
    }

    fn range_for_rename(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Task<Result<Option<Range<text::Anchor>>>> {
        let Some(this) = self.upgrade() else {
            return Task::ready(Ok(None));
        };

        this.update(cx, |project, cx| {
            let buffer = buffer.clone();
            let task = project.prepare_rename(buffer.clone(), position, cx);
            cx.spawn(async move |_, cx| {
                Ok(match task.await? {
                    PrepareRenameResponse::Success(range) => Some(range),
                    PrepareRenameResponse::InvalidPosition => None,
                    PrepareRenameResponse::OnlyUnpreparedRenameSupported => {
                        buffer.read_with(cx, |buffer, _| {
                            let snapshot = buffer.snapshot();
                            let (range, kind) = snapshot.surrounding_word(position, None);
                            if kind != Some(CharKind::Word) {
                                return None;
                            }
                            Some(
                                snapshot.anchor_before(range.start)
                                    ..snapshot.anchor_after(range.end),
                            )
                        })
                    }
                })
            })
        })
    }

    fn perform_rename(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        new_name: String,
        cx: &mut App,
    ) -> Option<Task<Result<ProjectTransaction>>> {
        self.update(cx, |project, cx| {
            project.perform_rename(buffer.clone(), position, new_name, cx)
        })
        .ok()
    }
}

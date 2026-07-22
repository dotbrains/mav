use super::*;

impl Project {
    pub fn reload_buffers(
        &self,
        buffers: HashSet<Entity<Buffer>>,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.reload_buffers(buffers, push_to_history, cx)
        })
    }

    pub fn reload_images(
        &self,
        images: HashSet<Entity<ImageItem>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.image_store
            .update(cx, |image_store, cx| image_store.reload_images(images, cx))
    }

    pub fn format(
        &mut self,
        buffers: HashSet<Entity<Buffer>>,
        target: LspFormatTarget,
        push_to_history: bool,
        trigger: lsp_store::FormatTrigger,
        cx: &mut Context<Project>,
    ) -> Task<anyhow::Result<ProjectTransaction>> {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.format(buffers, target, push_to_history, trigger, cx)
        })
    }

    pub fn supports_range_formatting(&self, buffer: &Entity<Buffer>, cx: &App) -> bool {
        self.lsp_store
            .read(cx)
            .supports_range_formatting(buffer, cx)
    }

    pub fn definitions<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.definitions(buffer, position, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    pub fn workspace_definitions<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.workspace_definitions(buffer, position, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    pub fn declarations<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.declarations(buffer, position, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    pub fn type_definitions<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.type_definitions(buffer, position, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    pub fn workspace_type_definitions<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.workspace_type_definitions(buffer, position, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    pub fn implementations<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.implementations(buffer, position, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    pub fn references<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<Location>>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.references(buffer, position, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    pub fn document_highlights<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<DocumentHighlight>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        self.request_lsp(
            buffer.clone(),
            LanguageServerToQuery::FirstCapable,
            GetDocumentHighlights { position },
            cx,
        )
    }

    pub fn document_symbols(
        &mut self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<DocumentSymbol>>> {
        self.request_lsp(
            buffer.clone(),
            LanguageServerToQuery::FirstCapable,
            GetDocumentSymbols,
            cx,
        )
    }

    pub fn symbols(&self, query: &str, cx: &mut Context<Self>) -> Task<Result<Vec<Symbol>>> {
        self.lsp_store
            .update(cx, |lsp_store, cx| lsp_store.symbols(query, cx))
    }

    pub fn open_buffer_for_symbol(
        &mut self,
        symbol: &Symbol,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.open_buffer_for_symbol(symbol, cx)
        })
    }

    pub fn open_server_settings(&mut self, cx: &mut Context<Self>) -> Task<Result<Entity<Buffer>>> {
        let guard = self.retain_remotely_created_models(cx);
        let Some(remote) = self.remote_client.as_ref() else {
            return Task::ready(Err(anyhow!("not an ssh project")));
        };

        let proto_client = remote.read(cx).proto_client();

        cx.spawn(async move |project, cx| {
            let buffer = proto_client
                .request(proto::OpenServerSettings {
                    project_id: REMOTE_SERVER_PROJECT_ID,
                })
                .await?;

            let buffer = project
                .update(cx, |project, cx| {
                    project.buffer_store.update(cx, |buffer_store, cx| {
                        anyhow::Ok(
                            buffer_store
                                .wait_for_remote_buffer(BufferId::new(buffer.buffer_id)?, cx),
                        )
                    })
                })??
                .await;

            drop(guard);
            buffer
        })
    }

    pub fn open_local_buffer_via_lsp(
        &mut self,
        abs_path: lsp::Uri,
        language_server_id: LanguageServerId,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.open_local_buffer_via_lsp(abs_path, language_server_id, cx)
        })
    }

    pub fn hover<T: ToPointUtf16>(
        &self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<Hover>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        self.lsp_store
            .update(cx, |lsp_store, cx| lsp_store.hover(buffer, position, cx))
    }

    pub fn linked_edits(
        &self,
        buffer: &Entity<Buffer>,
        position: Anchor,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<Range<Anchor>>>> {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.linked_edits(buffer, position, cx)
        })
    }

    pub fn completions<T: ToOffset + ToPointUtf16>(
        &self,
        buffer: &Entity<Buffer>,
        position: T,
        context: CompletionContext,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<CompletionResponse>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.completions(buffer, position, context, cx)
        })
    }

    pub fn code_actions<T: Clone + ToOffset>(
        &mut self,
        buffer_handle: &Entity<Buffer>,
        range: Range<T>,
        kinds: Option<Vec<CodeActionKind>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<CodeAction>>>> {
        let buffer = buffer_handle.read(cx);
        let range = buffer.anchor_before(range.start)..buffer.anchor_before(range.end);
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.code_actions(buffer_handle, range, kinds, cx)
        })
    }

    pub fn apply_code_action(
        &self,
        buffer_handle: Entity<Buffer>,
        action: CodeAction,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.apply_code_action(buffer_handle, action, push_to_history, cx)
        })
    }

    pub fn apply_code_action_kind(
        &self,
        buffers: HashSet<Entity<Buffer>>,
        kind: CodeActionKind,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.apply_code_action_kind(buffers, kind, push_to_history, cx)
        })
    }

    pub fn prepare_rename<T: ToPointUtf16>(
        &mut self,
        buffer: Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Result<PrepareRenameResponse>> {
        let position = position.to_point_utf16(buffer.read(cx));
        self.request_lsp(
            buffer,
            LanguageServerToQuery::FirstCapable,
            PrepareRename { position },
            cx,
        )
    }

    pub fn perform_rename<T: ToPointUtf16>(
        &mut self,
        buffer: Entity<Buffer>,
        position: T,
        new_name: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        let push_to_history = true;
        let position = position.to_point_utf16(buffer.read(cx));
        self.request_lsp(
            buffer,
            LanguageServerToQuery::FirstCapable,
            PerformRename {
                position,
                new_name,
                push_to_history,
            },
            cx,
        )
    }

    pub fn on_type_format<T: ToPointUtf16>(
        &mut self,
        buffer: Entity<Buffer>,
        position: T,
        trigger: String,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.on_type_format(buffer, position, trigger, push_to_history, cx)
        })
    }

    pub fn inline_values(
        &mut self,
        session: Entity<Session>,
        active_stack_frame: ActiveStackFrame,
        buffer_handle: Entity<Buffer>,
        range: Range<text::Anchor>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Vec<InlayHint>>> {
        let snapshot = buffer_handle.read(cx).snapshot();

        let captures =
            snapshot.debug_variables_query(Anchor::min_for_buffer(snapshot.remote_id())..range.end);

        let row = snapshot
            .summary_for_anchor::<text::PointUtf16>(&range.end)
            .row as usize;

        let inline_value_locations = provide_inline_values(captures, &snapshot, row);

        let stack_frame_id = active_stack_frame.stack_frame_id;
        cx.spawn(async move |this, cx| {
            this.update(cx, |project, cx| {
                project.dap_store().update(cx, |dap_store, cx| {
                    dap_store.resolve_inline_value_locations(
                        session,
                        stack_frame_id,
                        buffer_handle,
                        inline_value_locations,
                        cx,
                    )
                })
            })?
            .await
        })
    }
}

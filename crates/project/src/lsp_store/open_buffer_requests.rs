use super::*;

impl LspStore {
    pub fn open_buffer_for_symbol(
        &mut self,
        symbol: &Symbol,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        if let Some((client, project_id)) = self.upstream_client() {
            let request = client.request(proto::OpenBufferForSymbol {
                project_id,
                symbol: Some(Self::serialize_symbol(symbol)),
            });
            cx.spawn(async move |this, cx| {
                let response = request.await?;
                let buffer_id = BufferId::new(response.buffer_id)?;
                this.update(cx, |this, cx| this.wait_for_remote_buffer(buffer_id, cx))?
                    .await
            })
        } else if let Some(local) = self.as_local() {
            let is_valid = local.language_server_ids.iter().any(|(seed, state)| {
                seed.worktree_id == symbol.source_worktree_id
                    && state.id == symbol.source_language_server_id
                    && symbol.language_server_name == seed.name
            });
            if !is_valid {
                return Task::ready(Err(anyhow!(
                    "language server for worktree and language not found"
                )));
            };

            let symbol_abs_path = match &symbol.path {
                SymbolLocation::InProject(project_path) => self
                    .worktree_store
                    .read(cx)
                    .absolutize(&project_path, cx)
                    .context("no such worktree"),
                SymbolLocation::OutsideProject {
                    abs_path,
                    signature: _,
                } => Ok(abs_path.to_path_buf()),
            };
            let symbol_abs_path = match symbol_abs_path {
                Ok(abs_path) => abs_path,
                Err(err) => return Task::ready(Err(err)),
            };
            let symbol_uri = if let Ok(uri) = lsp::Uri::from_file_path(symbol_abs_path) {
                uri
            } else {
                return Task::ready(Err(anyhow!("invalid symbol path")));
            };

            self.open_local_buffer_via_lsp(symbol_uri, symbol.source_language_server_id, cx)
        } else {
            Task::ready(Err(anyhow!("no upstream client or local store")))
        }
    }

    pub(crate) fn open_local_buffer_via_lsp(
        &mut self,
        abs_path: lsp::Uri,
        language_server_id: LanguageServerId,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        let path_style = self.worktree_store.read(cx).path_style();
        cx.spawn(async move |lsp_store, cx| {
            // Escape percent-encoded string.
            let current_scheme = abs_path.scheme().to_owned();
            // Uri is immutable, so we can't modify the scheme

            let abs_path = abs_path
                .to_file_path_ext(path_style)
                .map_err(|()| anyhow!("can't convert URI to path"))?;
            let p = abs_path.clone();
            let yarn_worktree = lsp_store
                .update(cx, move |lsp_store, cx| match lsp_store.as_local() {
                    Some(local_lsp_store) => local_lsp_store.yarn.update(cx, |_, cx| {
                        cx.spawn(async move |this, cx| {
                            let t = this
                                .update(cx, |this, cx| this.process_path(&p, &current_scheme, cx))
                                .ok()?;
                            t.await
                        })
                    }),
                    None => Task::ready(None),
                })?
                .await;
            let (worktree_root_target, known_relative_path) =
                if let Some((zip_root, relative_path)) = yarn_worktree {
                    (zip_root, Some(relative_path))
                } else {
                    (Arc::<Path>::from(abs_path.as_path()), None)
                };
            let worktree = lsp_store.update(cx, |lsp_store, cx| {
                lsp_store.worktree_store.update(cx, |worktree_store, cx| {
                    worktree_store.find_worktree(&worktree_root_target, cx)
                })
            })?;
            let (worktree, relative_path, source_ws) = if let Some(result) = worktree {
                let relative_path = known_relative_path.unwrap_or_else(|| result.1.clone());
                (result.0, relative_path, None)
            } else {
                let worktree = lsp_store
                    .update(cx, |lsp_store, cx| {
                        lsp_store.worktree_store.update(cx, |worktree_store, cx| {
                            worktree_store.create_worktree(&worktree_root_target, false, cx)
                        })
                    })?
                    .await?;
                let worktree_root = worktree.read_with(cx, |worktree, _| worktree.abs_path());
                let source_ws = if worktree.read_with(cx, |worktree, _| worktree.is_local()) {
                    lsp_store
                        .update(cx, |lsp_store, cx| {
                            if let Some(local) = lsp_store.as_local_mut() {
                                local.register_language_server_for_invisible_worktree(
                                    &worktree,
                                    language_server_id,
                                    cx,
                                )
                            }
                            match lsp_store.language_server_statuses.get(&language_server_id) {
                                Some(status) => status.worktree,
                                None => None,
                            }
                        })
                        .ok()
                        .flatten()
                        .zip(Some(worktree_root.clone()))
                } else {
                    None
                };
                let relative_path = if let Some(known_path) = known_relative_path {
                    known_path
                } else {
                    RelPath::new(abs_path.strip_prefix(worktree_root)?, PathStyle::local())?
                        .into_arc()
                };
                (worktree, relative_path, source_ws)
            };
            let project_path = ProjectPath {
                worktree_id: worktree.read_with(cx, |worktree, _| worktree.id()),
                path: relative_path,
            };
            let buffer = lsp_store
                .update(cx, |lsp_store, cx| {
                    lsp_store.buffer_store().update(cx, |buffer_store, cx| {
                        buffer_store.open_buffer(project_path, cx)
                    })
                })?
                .await?;
            // we want to adhere to the read-only settings of the worktree we came from in case we opened an invisible one
            if let Some((source_ws, worktree_root)) = source_ws {
                buffer.update(cx, |buffer, cx| {
                    let settings = WorktreeSettings::get(
                        Some(
                            (&ProjectPath {
                                worktree_id: source_ws,
                                path: Arc::from(RelPath::empty()),
                            })
                                .into(),
                        ),
                        cx,
                    );
                    let is_read_only = settings.is_std_path_read_only(&worktree_root);
                    if is_read_only {
                        buffer.set_capability(Capability::ReadOnly, cx);
                    }
                });
            }
            Ok(buffer)
        })
    }
}

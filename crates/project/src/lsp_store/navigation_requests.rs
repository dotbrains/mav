use super::*;

impl LspStore {
    pub fn definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.definitions_with_filter(buffer, position, false, cx)
    }

    pub fn workspace_definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.definitions_with_filter(buffer, position, true, cx)
    }

    pub(super) fn definitions_with_filter(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        workspace_only: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetDefinitions {
                position,
                workspace_only,
            };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }

            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();

            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetDefinitions {
                        position,
                        workspace_only,
                    }
                    .response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let definitions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetDefinitions {
                    position,
                    workspace_only,
                },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    definitions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, definitions)| definitions)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn declarations(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetDeclarations { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetDeclarations { position }.response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let declarations_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetDeclarations { position },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    declarations_task
                        .await
                        .into_iter()
                        .flat_map(|(_, declarations)| declarations)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn type_definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.type_definitions_with_filter(buffer, position, false, cx)
    }

    pub fn workspace_type_definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.type_definitions_with_filter(buffer, position, true, cx)
    }

    pub(super) fn type_definitions_with_filter(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        workspace_only: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetTypeDefinitions {
                position,
                workspace_only,
            };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetTypeDefinitions {
                        position,
                        workspace_only,
                    }
                    .response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let type_definitions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetTypeDefinitions {
                    position,
                    workspace_only,
                },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    type_definitions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, type_definitions)| type_definitions)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn implementations(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetImplementations { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }

            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetImplementations { position }.response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let implementations_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetImplementations { position },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    implementations_task
                        .await
                        .into_iter()
                        .flat_map(|(_, implementations)| implementations)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn references(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<Location>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetReferences { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }

            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };

                let locations = join_all(responses.payload.into_iter().map(|lsp_response| {
                    GetReferences { position }.response_from_proto(
                        lsp_response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<Vec<_>>>>()?
                .into_iter()
                .flatten()
                .dedup()
                .collect();
                Ok(Some(locations))
            })
        } else {
            let references_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetReferences { position },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    references_task
                        .await
                        .into_iter()
                        .flat_map(|(_, references)| references)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn code_actions(
        &mut self,
        buffer: &Entity<Buffer>,
        range: Range<Anchor>,
        kinds: Option<Vec<CodeActionKind>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<CodeAction>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetCodeActions {
                range: range.clone(),
                kinds: kinds.clone(),
            };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetCodeActions {
                        range: range.clone(),
                        kinds: kinds.clone(),
                    }
                    .response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .collect(),
                ))
            })
        } else {
            let all_actions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(range.start),
                GetCodeActions { range, kinds },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    all_actions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, actions)| actions)
                        .collect(),
                ))
            })
        }
    }
}

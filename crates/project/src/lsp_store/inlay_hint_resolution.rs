use super::*;

impl LspStore {
    pub fn resolved_hint(
        &mut self,
        buffer_id: BufferId,
        id: InlayId,
        cx: &mut Context<Self>,
    ) -> Option<ResolvedHint> {
        let buffer = self.buffer_store.read(cx).get(buffer_id)?;

        let lsp_data = self.lsp_data.get_mut(&buffer_id)?;
        let buffer_lsp_hints = &mut lsp_data.inlay_hints;
        let hint = buffer_lsp_hints.hint_for_id(id)?.clone();
        let (server_id, resolve_data) = match &hint.resolve_state {
            ResolveState::Resolved => return Some(ResolvedHint::Resolved(hint)),
            ResolveState::Resolving => {
                return Some(ResolvedHint::Resolving(
                    buffer_lsp_hints.hint_resolves.get(&id)?.clone(),
                ));
            }
            ResolveState::CanResolve(server_id, resolve_data) => (*server_id, resolve_data.clone()),
        };

        let resolve_task = self.resolve_inlay_hint(hint, buffer, server_id, cx);
        let buffer_lsp_hints = &mut self.lsp_data.get_mut(&buffer_id)?.inlay_hints;
        let previous_task = buffer_lsp_hints.hint_resolves.insert(
            id,
            cx.spawn(async move |lsp_store, cx| {
                let resolved_hint = resolve_task.await;
                lsp_store
                    .update(cx, |lsp_store, _| {
                        if let Some(old_inlay_hint) = lsp_store
                            .lsp_data
                            .get_mut(&buffer_id)
                            .and_then(|buffer_lsp_data| buffer_lsp_data.inlay_hints.hint_for_id(id))
                        {
                            match resolved_hint {
                                Ok(resolved_hint) => {
                                    *old_inlay_hint = resolved_hint;
                                }
                                Err(e) => {
                                    old_inlay_hint.resolve_state =
                                        ResolveState::CanResolve(server_id, resolve_data);
                                    log::error!("Inlay hint resolve failed: {e:#}");
                                }
                            }
                        }
                    })
                    .ok();
            })
            .shared(),
        );
        debug_assert!(
            previous_task.is_none(),
            "Did not change hint's resolve state after spawning its resolve"
        );
        buffer_lsp_hints.hint_for_id(id)?.resolve_state = ResolveState::Resolving;
        None
    }
}

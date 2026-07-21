use super::*;

impl LspStore {
    pub(crate) fn linked_edits(
        &mut self,
        buffer: &Entity<Buffer>,
        position: Anchor,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<Range<Anchor>>>> {
        let snapshot = buffer.read(cx).snapshot();
        let scope = snapshot.language_scope_at(position);
        let Some(server_id) = self
            .as_local()
            .and_then(|local| {
                buffer.update(cx, |buffer, cx| {
                    local
                        .language_servers_for_buffer(buffer, cx)
                        .filter(|(_, server)| {
                            LinkedEditingRange::check_server_capabilities(server.capabilities())
                        })
                        .filter(|(adapter, _)| {
                            scope
                                .as_ref()
                                .map(|scope| scope.language_allowed(&adapter.name))
                                .unwrap_or(true)
                        })
                        .map(|(_, server)| LanguageServerToQuery::Other(server.server_id()))
                        .next()
                })
            })
            .or_else(|| {
                self.upstream_client()
                    .is_some()
                    .then_some(LanguageServerToQuery::FirstCapable)
            })
            .filter(|_| {
                maybe!({
                    buffer.read(cx).language_at(position)?;
                    Some(
                        LanguageSettings::for_buffer_at(&buffer.read(cx), position, cx)
                            .linked_edits,
                    )
                }) == Some(true)
            })
        else {
            return Task::ready(Ok(Vec::new()));
        };

        self.request_lsp(
            buffer.clone(),
            server_id,
            LinkedEditingRange { position },
            cx,
        )
    }
}

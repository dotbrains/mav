use super::*;

impl LspStore {
    pub(super) async fn handle_pull_workspace_diagnostics(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::PullWorkspaceDiagnostics>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let server_id = LanguageServerId::from_proto(envelope.payload.server_id);
        lsp_store.update(&mut cx, |lsp_store, _| {
            lsp_store.pull_workspace_diagnostics(server_id);
        });
        Ok(proto::Ack {})
    }
}

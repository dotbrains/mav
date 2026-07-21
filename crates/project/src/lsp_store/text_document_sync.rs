use super::*;

impl LspStore {
    pub(super) fn take_text_document_sync_options(
        capabilities: &mut lsp::ServerCapabilities,
    ) -> lsp::TextDocumentSyncOptions {
        match capabilities.text_document_sync.take() {
            Some(lsp::TextDocumentSyncCapability::Options(sync_options)) => sync_options,
            Some(lsp::TextDocumentSyncCapability::Kind(sync_kind)) => {
                let mut sync_options = lsp::TextDocumentSyncOptions::default();
                sync_options.change = Some(sync_kind);
                sync_options
            }
            None => lsp::TextDocumentSyncOptions::default(),
        }
    }
}

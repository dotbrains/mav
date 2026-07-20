use super::*;

pub(super) struct LspBufferSnapshot {
    pub(super) version: i32,
    pub(super) snapshot: TextBufferSnapshot,
}

pub(super) fn include_text(server: &lsp::LanguageServer) -> Option<bool> {
    match server.capabilities().text_document_sync.as_ref()? {
        lsp::TextDocumentSyncCapability::Options(opts) => match opts.save.as_ref()? {
            // Server wants didSave but didn't specify includeText.
            lsp::TextDocumentSyncSaveOptions::Supported(true) => Some(false),
            // Server doesn't want didSave at all.
            lsp::TextDocumentSyncSaveOptions::Supported(false) => None,
            // Server provided SaveOptions.
            lsp::TextDocumentSyncSaveOptions::SaveOptions(save_options) => {
                Some(save_options.include_text.unwrap_or(false))
            }
        },
        // We do not have any save info. Kind affects didChange only.
        lsp::TextDocumentSyncCapability::Kind(_) => None,
    }
}

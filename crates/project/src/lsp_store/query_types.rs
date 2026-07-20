use crate::InlayHint;
use futures::future::Shared;
use gpui::Task;
use lsp::LanguageServerId;

#[derive(Debug)]
pub enum LanguageServerToQuery {
    /// Query language servers in order of users preference, up until one capable of handling the request is found.
    FirstCapable,
    /// Query a specific language server.
    Other(LanguageServerId),
}

pub enum ResolvedHint {
    Resolved(InlayHint),
    Resolving(Shared<Task<()>>),
}

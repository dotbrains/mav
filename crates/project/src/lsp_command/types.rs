use super::*;

#[derive(Debug)]
pub(crate) struct PrepareRename {
    pub position: PointUtf16,
}

#[derive(Debug)]
pub(crate) struct PerformRename {
    pub position: PointUtf16,
    pub new_name: String,
    pub push_to_history: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct GetDefinitions {
    pub position: PointUtf16,
    pub workspace_only: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GetDeclarations {
    pub position: PointUtf16,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GetTypeDefinitions {
    pub position: PointUtf16,
    pub workspace_only: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GetImplementations {
    pub position: PointUtf16,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct GetReferences {
    pub position: PointUtf16,
}

#[derive(Debug)]
pub(crate) struct GetDocumentHighlights {
    pub position: PointUtf16,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct GetDocumentSymbols;

#[derive(Clone, Debug)]
pub(crate) struct GetSignatureHelp {
    pub position: PointUtf16,
}

#[derive(Clone, Debug)]
pub(crate) struct GetHover {
    pub position: PointUtf16,
}

#[derive(Debug)]
pub(crate) struct GetCompletions {
    pub position: PointUtf16,
    pub context: CompletionContext,
    pub server_id: Option<lsp::LanguageServerId>,
}

#[derive(Clone, Debug)]
pub(crate) struct GetCodeActions {
    pub range: Range<Anchor>,
    pub kinds: Option<Vec<lsp::CodeActionKind>>,
}

#[derive(Debug)]
pub(crate) struct OnTypeFormatting {
    pub position: PointUtf16,
    pub trigger: String,
    pub options: lsp::FormattingOptions,
    pub push_to_history: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct InlayHints {
    pub range: Range<Anchor>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SemanticTokensFull {
    pub for_server: Option<LanguageServerId>,
}

#[derive(Debug, Clone)]
pub(crate) struct SemanticTokensDelta {
    pub previous_result_id: SharedString,
}

#[derive(Debug)]
pub(crate) enum SemanticTokensResponse {
    Full {
        data: Vec<u32>,
        result_id: Option<SharedString>,
    },
    Delta {
        edits: Vec<SemanticTokensEdit>,
        result_id: Option<SharedString>,
    },
}

impl Default for SemanticTokensResponse {
    fn default() -> Self {
        Self::Delta {
            edits: Vec::new(),
            result_id: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct SemanticTokensEdit {
    pub start: u32,
    pub delete_count: u32,
    pub data: Vec<u32>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct GetCodeLens;

#[derive(Debug, Copy, Clone)]
pub(crate) struct GetDocumentColor;

#[derive(Debug, Copy, Clone)]
pub(crate) struct GetFoldingRanges;

#[derive(Debug, Copy, Clone)]
pub(crate) struct GetDocumentLinks;

impl GetCodeLens {
    pub(crate) fn can_resolve_lens(capabilities: &ServerCapabilities) -> bool {
        capabilities
            .code_lens_provider
            .as_ref()
            .and_then(|code_lens_options| code_lens_options.resolve_provider)
            .unwrap_or(false)
    }
}

#[derive(Debug)]
pub(crate) struct LinkedEditingRange {
    pub position: Anchor,
}

#[derive(Clone, Debug)]
pub struct GetDocumentDiagnostics {
    /// We cannot blindly rely on server's capabilities.diagnostic_provider, as they're a singular field, whereas
    /// a server can register multiple diagnostic providers post-mortem.
    pub registration_id: Option<SharedString>,
    pub identifier: Option<SharedString>,
    pub previous_result_id: Option<SharedString>,
}

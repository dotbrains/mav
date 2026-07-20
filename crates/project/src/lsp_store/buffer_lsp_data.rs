use super::*;

#[derive(Debug)]
pub struct BufferLspData {
    pub(super) buffer_version: Global,
    pub(super) document_colors: Option<DocumentColorData>,
    pub(super) code_lens: Option<CodeLensData>,
    pub(super) semantic_tokens: Option<SemanticTokensData>,
    pub(super) folding_ranges: Option<FoldingRangeData>,
    pub(super) document_links: Option<DocumentLinksData>,
    pub(super) document_symbols: Option<DocumentSymbolsData>,
    pub(super) inlay_hints: BufferInlayHints,
    pub(super) lsp_requests: HashMap<LspKey, HashMap<LspRequestId, Task<()>>>,
    pub(super) chunk_lsp_requests: HashMap<LspKey, HashMap<RowChunk, LspRequestId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct LspKey {
    pub(super) request_type: TypeId,
    pub(super) server_queried: Option<LanguageServerId>,
}

impl BufferLspData {
    pub(super) fn new(buffer: &Entity<Buffer>, cx: &mut App) -> Self {
        Self {
            buffer_version: buffer.read(cx).version(),
            document_colors: None,
            code_lens: None,
            semantic_tokens: None,
            folding_ranges: None,
            document_links: None,
            document_symbols: None,
            inlay_hints: BufferInlayHints::new(buffer, cx),
            lsp_requests: HashMap::default(),
            chunk_lsp_requests: HashMap::default(),
        }
    }

    pub(super) fn remove_server_data(&mut self, for_server: LanguageServerId) {
        if let Some(document_colors) = &mut self.document_colors {
            document_colors.remove_server_data(for_server);
        }

        if let Some(code_lens) = &mut self.code_lens {
            code_lens.remove_server_data(for_server);
        }

        self.inlay_hints.remove_server_data(for_server);

        if let Some(semantic_tokens) = &mut self.semantic_tokens {
            semantic_tokens.remove_server_data(for_server);
        }

        if let Some(folding_ranges) = &mut self.folding_ranges {
            folding_ranges.ranges.remove(&for_server);
        }

        if let Some(document_links) = &mut self.document_links {
            document_links.remove_server_data(for_server);
        }

        if let Some(document_symbols) = &mut self.document_symbols {
            document_symbols.remove_server_data(for_server);
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn inlay_hints(&self) -> &BufferInlayHints {
        &self.inlay_hints
    }
}

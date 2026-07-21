use crate::{Completion, CompletionSource, color_extractor};
use gpui::Hsla;
use lsp::CompletionItemKind;

impl Completion {
    pub fn kind(&self) -> Option<CompletionItemKind> {
        self.source
            // `lsp::CompletionListItemDefaults` has no `kind` field
            .lsp_completion(false)
            .and_then(|lsp_completion| lsp_completion.kind)
    }

    pub fn label(&self) -> Option<String> {
        self.source
            .lsp_completion(false)
            .map(|lsp_completion| lsp_completion.label.clone())
    }

    /// A key that can be used to sort completions when displaying
    /// them to the user.
    pub fn sort_key(&self) -> (usize, &str) {
        const DEFAULT_KIND_KEY: usize = 4;
        let kind_key = self
            .kind()
            .and_then(|lsp_completion_kind| match lsp_completion_kind {
                lsp::CompletionItemKind::KEYWORD => Some(0),
                lsp::CompletionItemKind::VARIABLE => Some(1),
                lsp::CompletionItemKind::CONSTANT => Some(2),
                lsp::CompletionItemKind::PROPERTY => Some(3),
                _ => None,
            })
            .unwrap_or(DEFAULT_KIND_KEY);
        (kind_key, self.label.filter_text())
    }

    /// Whether this completion is a snippet.
    pub fn is_snippet_kind(&self) -> bool {
        matches!(
            &self.source,
            CompletionSource::Lsp { lsp_completion, .. }
            if lsp_completion.kind == Some(CompletionItemKind::SNIPPET)
        )
    }

    /// Whether this completion is a snippet or snippet-style LSP completion.
    pub fn is_snippet(&self) -> bool {
        self.source
            // `lsp::CompletionListItemDefaults` has `insert_text_format` field
            .lsp_completion(true)
            .is_some_and(|lsp_completion| {
                lsp_completion.insert_text_format == Some(lsp::InsertTextFormat::SNIPPET)
            })
    }

    /// Returns the corresponding color for this completion.
    ///
    /// Will return `None` if this completion's kind is not [`CompletionItemKind::COLOR`].
    pub fn color(&self) -> Option<Hsla> {
        // `lsp::CompletionListItemDefaults` has no `kind` field
        let lsp_completion = self.source.lsp_completion(false)?;
        if lsp_completion.kind? == CompletionItemKind::COLOR {
            return color_extractor::extract_color(&lsp_completion);
        }
        None
    }
}

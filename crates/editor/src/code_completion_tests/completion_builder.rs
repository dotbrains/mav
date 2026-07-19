use language::CodeLabel;
use lsp::{CompletionItem, CompletionItemKind, LanguageServerId};
use project::{Completion, CompletionSource};
use text::{Anchor, BufferId};

pub(super) struct CompletionBuilder;

impl CompletionBuilder {
    pub(super) fn constant(label: &str, filter_text: Option<&str>, sort_text: &str) -> Completion {
        Self::new(
            label,
            filter_text,
            sort_text,
            Some(CompletionItemKind::CONSTANT),
        )
    }

    pub(super) fn function(label: &str, filter_text: Option<&str>, sort_text: &str) -> Completion {
        Self::new(
            label,
            filter_text,
            sort_text,
            Some(CompletionItemKind::FUNCTION),
        )
    }

    pub(super) fn method(label: &str, filter_text: Option<&str>, sort_text: &str) -> Completion {
        Self::new(
            label,
            filter_text,
            sort_text,
            Some(CompletionItemKind::METHOD),
        )
    }

    pub(super) fn variable(label: &str, filter_text: Option<&str>, sort_text: &str) -> Completion {
        Self::new(
            label,
            filter_text,
            sort_text,
            Some(CompletionItemKind::VARIABLE),
        )
    }

    pub(super) fn snippet(label: &str, filter_text: Option<&str>, sort_text: &str) -> Completion {
        Self::new(
            label,
            filter_text,
            sort_text,
            Some(CompletionItemKind::SNIPPET),
        )
    }

    pub(super) fn new(
        label: &str,
        filter_text: Option<&str>,
        sort_text: &str,
        kind: Option<CompletionItemKind>,
    ) -> Completion {
        Completion {
            replace_range: Anchor::min_max_range_for_buffer(BufferId::new(1).unwrap()),
            new_text: label.to_string(),
            label: CodeLabel::plain(label.to_string(), filter_text),
            documentation: None,
            source: CompletionSource::Lsp {
                insert_range: None,
                server_id: LanguageServerId(0),
                lsp_completion: Box::new(CompletionItem {
                    label: label.to_string(),
                    kind,
                    sort_text: Some(sort_text.to_string()),
                    filter_text: filter_text.map(|text| text.to_string()),
                    ..Default::default()
                }),
                lsp_defaults: None,
                resolved: false,
            },
            icon_path: None,
            icon_color: None,
            insert_text_mode: None,
            confirm: None,
            match_start: None,
            snippet_deduplication_key: None,
            group: None,
        }
    }
}

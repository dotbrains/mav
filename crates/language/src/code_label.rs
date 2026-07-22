use super::*;

pub trait CodeLabelExt {
    fn fallback_for_completion(
        item: &lsp::CompletionItem,
        language: Option<&Language>,
    ) -> CodeLabel;
}

impl CodeLabelExt for CodeLabel {
    fn fallback_for_completion(
        item: &lsp::CompletionItem,
        language: Option<&Language>,
    ) -> CodeLabel {
        let highlight_id = item.kind.and_then(|kind| {
            let grammar = language?.grammar()?;
            use lsp::CompletionItemKind as Kind;
            match kind {
                Kind::CLASS => grammar.highlight_id_for_name("type"),
                Kind::CONSTANT => grammar.highlight_id_for_name("constant"),
                Kind::CONSTRUCTOR => grammar.highlight_id_for_name("constructor"),
                Kind::ENUM => grammar
                    .highlight_id_for_name("enum")
                    .or_else(|| grammar.highlight_id_for_name("type")),
                Kind::ENUM_MEMBER => grammar
                    .highlight_id_for_name("variant")
                    .or_else(|| grammar.highlight_id_for_name("property")),
                Kind::FIELD => grammar.highlight_id_for_name("property"),
                Kind::FUNCTION => grammar.highlight_id_for_name("function"),
                Kind::INTERFACE => grammar.highlight_id_for_name("type"),
                Kind::METHOD => grammar
                    .highlight_id_for_name("function.method")
                    .or_else(|| grammar.highlight_id_for_name("function")),
                Kind::OPERATOR => grammar.highlight_id_for_name("operator"),
                Kind::PROPERTY => grammar.highlight_id_for_name("property"),
                Kind::STRUCT => grammar.highlight_id_for_name("type"),
                Kind::VARIABLE => grammar.highlight_id_for_name("variable"),
                Kind::KEYWORD => grammar.highlight_id_for_name("keyword"),
                _ => None,
            }
        });

        let label = &item.label;
        let label_length = label.len();
        let runs = highlight_id
            .map(|highlight_id| vec![(0..label_length, highlight_id)])
            .unwrap_or_default();
        let text = if let Some(detail) = item.detail.as_deref().filter(|detail| detail != label) {
            format!("{label} {detail}")
        } else if let Some(description) = item
            .label_details
            .as_ref()
            .and_then(|label_details| label_details.description.as_deref())
            .filter(|description| description != label)
        {
            format!("{label} {description}")
        } else {
            label.clone()
        };
        let filter_range = item
            .filter_text
            .as_deref()
            .and_then(|filter| text.find(filter).map(|ix| ix..ix + filter.len()))
            .unwrap_or(0..label_length);
        CodeLabel {
            text,
            runs,
            filter_range,
        }
    }
}

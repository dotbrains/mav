use super::*;

pub(super) fn render_completion_kind_letter(
    kind: Option<CompletionItemKind>,
    item_ix: usize,
    style: &EditorStyle,
) -> AnyElement {
    let badge = div()
        .flex_none()
        .w(IconSize::XSmall.rems())
        .text_center()
        .text_size(rems_from_px(11.))
        .line_height(rems_from_px(14.));

    let Some(kind) = kind else {
        return badge.into_any_element();
    };
    let Some(letter) = completion_kind_letter(kind) else {
        return badge.into_any_element();
    };

    let color = completion_kind_highlight_name(kind)
        .and_then(|name| {
            style.syntax.style_for_name(name).or_else(|| {
                let (parent, _) = name.rsplit_once('.')?;
                style.syntax.style_for_name(parent)
            })
        })
        .and_then(|hl| hl.color);

    badge
        .id(("completion-kind", item_ix))
        .tooltip(Tooltip::text(completion_kind_name(kind)))
        .child(letter)
        .when_some(color, |element, color| element.text_color(color))
        .into_any_element()
}

pub(crate) fn completion_kind_name(kind: CompletionItemKind) -> &'static str {
    match kind {
        CompletionItemKind::TEXT => "Text",
        CompletionItemKind::METHOD => "Method",
        CompletionItemKind::FUNCTION => "Function",
        CompletionItemKind::CONSTRUCTOR => "Constructor",
        CompletionItemKind::FIELD => "Field",
        CompletionItemKind::VARIABLE => "Variable",
        CompletionItemKind::CLASS => "Class",
        CompletionItemKind::INTERFACE => "Interface",
        CompletionItemKind::MODULE => "Module",
        CompletionItemKind::PROPERTY => "Property",
        CompletionItemKind::UNIT => "Unit",
        CompletionItemKind::VALUE => "Value",
        CompletionItemKind::ENUM => "Enum",
        CompletionItemKind::KEYWORD => "Keyword",
        CompletionItemKind::SNIPPET => "Snippet",
        CompletionItemKind::COLOR => "Color",
        CompletionItemKind::FILE => "File",
        CompletionItemKind::REFERENCE => "Reference",
        CompletionItemKind::FOLDER => "Folder",
        CompletionItemKind::ENUM_MEMBER => "Enum Member",
        CompletionItemKind::CONSTANT => "Constant",
        CompletionItemKind::STRUCT => "Struct",
        CompletionItemKind::EVENT => "Event",
        CompletionItemKind::OPERATOR => "Operator",
        CompletionItemKind::TYPE_PARAMETER => "Type Parameter",
        _ => "Unknown",
    }
}

pub(crate) fn completion_kind_letter(kind: CompletionItemKind) -> Option<&'static str> {
    Some(match kind {
        CompletionItemKind::TEXT => "t",
        CompletionItemKind::METHOD => "m",
        CompletionItemKind::FUNCTION => "f",
        CompletionItemKind::CONSTRUCTOR => "C",
        CompletionItemKind::FIELD => "f",
        CompletionItemKind::VARIABLE => "v",
        CompletionItemKind::CLASS => "c",
        CompletionItemKind::INTERFACE => "i",
        CompletionItemKind::MODULE => "M",
        CompletionItemKind::PROPERTY => "p",
        CompletionItemKind::UNIT => "u",
        CompletionItemKind::VALUE => "v",
        CompletionItemKind::ENUM => "e",
        CompletionItemKind::KEYWORD => "k",
        CompletionItemKind::SNIPPET => "s",
        CompletionItemKind::COLOR => "c",
        CompletionItemKind::FILE => "F",
        CompletionItemKind::REFERENCE => "r",
        CompletionItemKind::FOLDER => "D",
        CompletionItemKind::ENUM_MEMBER => "e",
        CompletionItemKind::CONSTANT => "c",
        CompletionItemKind::STRUCT => "S",
        CompletionItemKind::EVENT => "E",
        CompletionItemKind::OPERATOR => "o",
        CompletionItemKind::TYPE_PARAMETER => "T",
        _ => return None,
    })
}

pub(crate) fn completion_kind_highlight_name(kind: CompletionItemKind) -> Option<&'static str> {
    Some(match kind {
        CompletionItemKind::CLASS => "type",
        CompletionItemKind::CONSTANT => "constant",
        CompletionItemKind::CONSTRUCTOR => "constructor",
        CompletionItemKind::ENUM => "enum",
        CompletionItemKind::ENUM_MEMBER => "variant",
        CompletionItemKind::FIELD => "property",
        CompletionItemKind::FUNCTION => "function",
        CompletionItemKind::INTERFACE => "type",
        CompletionItemKind::METHOD => "function.method",
        CompletionItemKind::MODULE => "namespace",
        CompletionItemKind::OPERATOR => "operator",
        CompletionItemKind::PROPERTY => "property",
        CompletionItemKind::STRUCT => "type",
        CompletionItemKind::TYPE_PARAMETER => "type",
        CompletionItemKind::VARIABLE => "variable",
        CompletionItemKind::KEYWORD => "keyword",
        CompletionItemKind::SNIPPET => "string",
        _ => return None,
    })
}

pub(super) fn exact_case_match_count(query: &str, string_match: &StringMatch) -> usize {
    let mut exact_matches = 0;
    let mut query_chars = query.chars();
    let mut next_query_char = query_chars.next();
    let mut matched_positions = string_match.positions.iter().copied().peekable();

    for (index, candidate_char) in string_match.string.char_indices() {
        if matched_positions.peek() == Some(&index) {
            let Some(query_char) = next_query_char else {
                break;
            };

            if query_char == candidate_char {
                exact_matches += 1;
            }

            matched_positions.next();
            next_query_char = query_chars.next();
        }
    }

    exact_matches
}

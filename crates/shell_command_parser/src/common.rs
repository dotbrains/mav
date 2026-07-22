use super::*;

pub(super) fn first_simple_command(program: &ast::Program) -> Option<&ast::SimpleCommand> {
    let complete_command = program.complete_commands.first()?;
    let compound_list_item = complete_command.0.first()?;
    let command = compound_list_item.0.first.seq.first()?;

    match command {
        ast::Command::Simple(simple_command) => Some(simple_command),
        _ => None,
    }
}

pub(super) fn update_display_bounds(
    start: &mut Option<usize>,
    end: &mut Option<usize>,
    word: &ast::Word,
) {
    if let Some(location) = word.location() {
        let word_start = location.start.index;
        let word_end = location.end.index;
        *start = Some(start.map_or(word_start, |current| current.min(word_start)));
        *end = Some(end.map_or(word_end, |current| current.max(word_end)));
    }
}

pub(super) enum NormalizedAssignment {
    Included(String),
    Skipped,
}

pub(super) fn normalize_assignment_for_command_prefix(
    assignment: &ast::Assignment,
    word: &ast::Word,
) -> Option<NormalizedAssignment> {
    let operator = if assignment.append { "+=" } else { "=" };
    let assignment_prefix = format!("{}{}", assignment.name, operator);

    match &assignment.value {
        ast::AssignmentValue::Scalar(value) => {
            let normalized_value = normalize_word(value)?;
            let raw_value = word.value.strip_prefix(&assignment_prefix)?;
            let rendered_value = if shell_value_requires_quoting(&normalized_value) {
                raw_value.to_string()
            } else {
                normalized_value
            };

            Some(NormalizedAssignment::Included(format!(
                "{assignment_prefix}{rendered_value}"
            )))
        }
        ast::AssignmentValue::Array(_) => Some(NormalizedAssignment::Skipped),
    }
}

fn shell_value_requires_quoting(value: &str) -> bool {
    value.chars().any(|character| {
        character.is_whitespace()
            || !matches!(
                character,
                'a'..='z'
                    | 'A'..='Z'
                    | '0'..='9'
                    | '_'
                    | '@'
                    | '%'
                    | '+'
                    | '='
                    | ':'
                    | ','
                    | '.'
                    | '/'
                    | '-'
            )
    })
}

/// Normalizes a shell word by stripping quoting syntax and returning the
/// semantic (unquoted) value. Returns `None` if word parsing fails.
pub(super) fn normalize_word(word: &ast::Word) -> Option<String> {
    let options = ParserOptions::default();
    let pieces = brush_parser::word::parse(&word.value, &options).ok()?;
    let mut result = String::new();
    for piece_with_source in &pieces {
        normalize_word_piece_into(
            &piece_with_source.piece,
            &word.value,
            piece_with_source.start_index,
            piece_with_source.end_index,
            &mut result,
        )?;
    }
    Some(result)
}

fn normalize_word_piece_into(
    piece: &WordPiece,
    raw_value: &str,
    start_index: usize,
    end_index: usize,
    result: &mut String,
) -> Option<()> {
    match piece {
        WordPiece::Text(text) => result.push_str(text),
        WordPiece::SingleQuotedText(text) => result.push_str(text),
        WordPiece::AnsiCQuotedText(text) => result.push_str(text),
        WordPiece::EscapeSequence(text) => {
            result.push_str(text.strip_prefix('\\').unwrap_or(text));
        }
        WordPiece::DoubleQuotedSequence(pieces)
        | WordPiece::GettextDoubleQuotedSequence(pieces) => {
            for inner in pieces {
                normalize_word_piece_into(
                    &inner.piece,
                    raw_value,
                    inner.start_index,
                    inner.end_index,
                    result,
                )?;
            }
        }
        WordPiece::TildePrefix(prefix) => {
            result.push('~');
            result.push_str(prefix);
        }
        // For parameter expansions, command substitutions, and arithmetic expressions,
        // preserve the original source text so that patterns like `\$HOME` continue
        // to match.
        WordPiece::ParameterExpansion(_)
        | WordPiece::CommandSubstitution(_)
        | WordPiece::BackquotedCommandSubstitution(_)
        | WordPiece::ArithmeticExpression(_) => {
            let source = raw_value.get(start_index..end_index)?;
            result.push_str(source);
        }
    }
    Some(())
}

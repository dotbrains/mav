use super::*;

pub(super) fn extract_commands_from_command_prefix(
    prefix: &ast::CommandPrefix,
    commands: &mut Vec<String>,
) -> Option<()> {
    for item in &prefix.0 {
        extract_commands_from_prefix_or_suffix_item(item, commands)?;
    }
    Some(())
}

pub(super) fn extract_commands_from_command_suffix(
    suffix: &ast::CommandSuffix,
    commands: &mut Vec<String>,
) -> Option<()> {
    for item in &suffix.0 {
        extract_commands_from_prefix_or_suffix_item(item, commands)?;
    }
    Some(())
}

pub(super) fn extract_commands_from_prefix_or_suffix_item(
    item: &ast::CommandPrefixOrSuffixItem,
    commands: &mut Vec<String>,
) -> Option<()> {
    match item {
        ast::CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
            extract_commands_from_io_redirect(redirect, commands)?;
        }
        ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, _word) => {
            extract_commands_from_assignment(assignment, commands)?;
        }
        ast::CommandPrefixOrSuffixItem::Word(word) => {
            extract_commands_from_word(word, commands)?;
        }
        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_kind, subshell) => {
            extract_commands_from_compound_list(&subshell.list, commands)?;
        }
    }
    Some(())
}

pub(super) fn extract_commands_from_io_redirect(
    redirect: &ast::IoRedirect,
    commands: &mut Vec<String>,
) -> Option<()> {
    match redirect {
        ast::IoRedirect::File(_fd, _kind, target) => match target {
            ast::IoFileRedirectTarget::ProcessSubstitution(_kind, subshell) => {
                extract_commands_from_compound_list(&subshell.list, commands)?;
            }
            ast::IoFileRedirectTarget::Filename(word) => {
                extract_commands_from_word(word, commands)?;
            }
            _ => {}
        },
        ast::IoRedirect::HereDocument(_fd, here_doc) => {
            if here_doc.requires_expansion {
                extract_commands_from_word(&here_doc.doc, commands)?;
            }
        }
        ast::IoRedirect::HereString(_fd, word) => {
            extract_commands_from_word(word, commands)?;
        }
        ast::IoRedirect::OutputAndError(word, _) => {
            extract_commands_from_word(word, commands)?;
        }
    }
    Some(())
}

pub(super) fn extract_commands_from_assignment(
    assignment: &ast::Assignment,
    commands: &mut Vec<String>,
) -> Option<()> {
    match &assignment.value {
        ast::AssignmentValue::Scalar(word) => {
            extract_commands_from_word(word, commands)?;
        }
        ast::AssignmentValue::Array(words) => {
            for (opt_word, word) in words {
                if let Some(w) = opt_word {
                    extract_commands_from_word(w, commands)?;
                }
                extract_commands_from_word(word, commands)?;
            }
        }
    }
    Some(())
}

pub(super) fn extract_commands_from_word(
    word: &ast::Word,
    commands: &mut Vec<String>,
) -> Option<()> {
    let options = ParserOptions::default();
    let pieces = brush_parser::word::parse(&word.value, &options).ok()?;
    for piece_with_source in pieces {
        extract_commands_from_word_piece(&piece_with_source.piece, commands)?;
    }
    Some(())
}

pub(super) fn extract_commands_from_word_piece(
    piece: &WordPiece,
    commands: &mut Vec<String>,
) -> Option<()> {
    match piece {
        WordPiece::CommandSubstitution(cmd_str)
        | WordPiece::BackquotedCommandSubstitution(cmd_str) => {
            let nested_commands = extract_commands(cmd_str)?;
            commands.extend(nested_commands);
        }
        WordPiece::DoubleQuotedSequence(pieces)
        | WordPiece::GettextDoubleQuotedSequence(pieces) => {
            for inner_piece_with_source in pieces {
                extract_commands_from_word_piece(&inner_piece_with_source.piece, commands)?;
            }
        }
        WordPiece::ArithmeticExpression(expr) => {
            // The arithmetic body may contain `$(...)` or `${...}` that bash will
            // evaluate before doing arithmetic. Re-parse to extract those.
            // We propagate parse failures with `?` so that callers fail closed
            // (treating the whole input as a parse failure) rather than silently
            // dropping commands hidden inside content brush couldn't tokenize.
            extract_commands_from_word_string(&expr.value, commands)?;
        }
        WordPiece::ParameterExpansion(expr) => {
            extract_commands_from_parameter_expr(expr, commands)?;
        }
        WordPiece::EscapeSequence(_)
        | WordPiece::SingleQuotedText(_)
        | WordPiece::Text(_)
        | WordPiece::AnsiCQuotedText(_)
        | WordPiece::TildePrefix(_) => {}
    }
    Some(())
}

/// Re-parses a string as a bash word and recurses into its pieces to extract
/// any nested command substitutions. Returns `None` (failing closed) if brush
/// cannot tokenize the input, so callers treat allowlist decisions about this
/// input as untrusted.
pub(super) fn extract_commands_from_word_string(s: &str, commands: &mut Vec<String>) -> Option<()> {
    let options = ParserOptions::default();
    let pieces = brush_parser::word::parse(s, &options).ok()?;
    for inner_piece in pieces {
        extract_commands_from_word_piece(&inner_piece.piece, commands)?;
    }
    Some(())
}

/// Recurses into the string-typed fields of a parameter expansion that bash
/// will subject to command substitution at expansion time, mirroring the
/// arithmetic expansion handling. Failing to extend this when adding new
/// `ParameterExpr` variants risks an allowlist bypass via e.g.
/// `${V:-$(curl evil)}`, `${V/pat/$(curl evil)}`, `${V:$(($(curl))):1}`.
pub(super) fn extract_commands_from_parameter_expr(
    expr: &brush_parser::word::ParameterExpr,
    commands: &mut Vec<String>,
) -> Option<()> {
    use brush_parser::word::ParameterExpr;
    match expr {
        ParameterExpr::Parameter { .. }
        | ParameterExpr::ParameterLength { .. }
        | ParameterExpr::Transform { .. }
        | ParameterExpr::VariableNames { .. }
        | ParameterExpr::MemberKeys { .. } => {}
        ParameterExpr::UseDefaultValues { default_value, .. }
        | ParameterExpr::AssignDefaultValues { default_value, .. } => {
            if let Some(value) = default_value {
                extract_commands_from_word_string(value, commands)?;
            }
        }
        ParameterExpr::IndicateErrorIfNullOrUnset { error_message, .. } => {
            if let Some(value) = error_message {
                extract_commands_from_word_string(value, commands)?;
            }
        }
        ParameterExpr::UseAlternativeValue {
            alternative_value, ..
        } => {
            if let Some(value) = alternative_value {
                extract_commands_from_word_string(value, commands)?;
            }
        }
        ParameterExpr::RemoveSmallestSuffixPattern { pattern, .. }
        | ParameterExpr::RemoveLargestSuffixPattern { pattern, .. }
        | ParameterExpr::RemoveSmallestPrefixPattern { pattern, .. }
        | ParameterExpr::RemoveLargestPrefixPattern { pattern, .. }
        | ParameterExpr::UppercaseFirstChar { pattern, .. }
        | ParameterExpr::UppercasePattern { pattern, .. }
        | ParameterExpr::LowercaseFirstChar { pattern, .. }
        | ParameterExpr::LowercasePattern { pattern, .. } => {
            if let Some(pattern) = pattern {
                extract_commands_from_word_string(pattern, commands)?;
            }
        }
        ParameterExpr::Substring { offset, length, .. } => {
            extract_commands_from_word_string(&offset.value, commands)?;
            if let Some(length) = length {
                extract_commands_from_word_string(&length.value, commands)?;
            }
        }
        ParameterExpr::ReplaceSubstring {
            pattern,
            replacement,
            ..
        } => {
            extract_commands_from_word_string(pattern, commands)?;
            if let Some(replacement) = replacement {
                extract_commands_from_word_string(replacement, commands)?;
            }
        }
    }
    Some(())
}

pub(super) fn extract_commands_from_compound_command(
    compound_command: &ast::CompoundCommand,
    commands: &mut Vec<String>,
) -> Option<usize> {
    match compound_command {
        ast::CompoundCommand::BraceGroup(brace_group) => {
            let body_start = commands.len();
            extract_commands_from_compound_list(&brace_group.list, commands)?;
            Some(body_start)
        }
        ast::CompoundCommand::Subshell(subshell) => {
            let body_start = commands.len();
            extract_commands_from_compound_list(&subshell.list, commands)?;
            Some(body_start)
        }
        ast::CompoundCommand::ForClause(for_clause) => {
            if let Some(words) = &for_clause.values {
                for word in words {
                    extract_commands_from_word(word, commands)?;
                }
            }
            let body_start = commands.len();
            extract_commands_from_do_group(&for_clause.body, commands)?;
            Some(body_start)
        }
        ast::CompoundCommand::CaseClause(case_clause) => {
            extract_commands_from_word(&case_clause.value, commands)?;
            let body_start = commands.len();
            for item in &case_clause.cases {
                if let Some(body) = &item.cmd {
                    extract_commands_from_compound_list(body, commands)?;
                }
            }
            Some(body_start)
        }
        ast::CompoundCommand::IfClause(if_clause) => {
            extract_commands_from_compound_list(&if_clause.condition, commands)?;
            let body_start = commands.len();
            extract_commands_from_compound_list(&if_clause.then, commands)?;
            if let Some(elses) = &if_clause.elses {
                for else_item in elses {
                    if let Some(condition) = &else_item.condition {
                        extract_commands_from_compound_list(condition, commands)?;
                    }
                    extract_commands_from_compound_list(&else_item.body, commands)?;
                }
            }
            Some(body_start)
        }
        ast::CompoundCommand::WhileClause(while_clause)
        | ast::CompoundCommand::UntilClause(while_clause) => {
            extract_commands_from_compound_list(&while_clause.0, commands)?;
            let body_start = commands.len();
            extract_commands_from_do_group(&while_clause.1, commands)?;
            Some(body_start)
        }
        ast::CompoundCommand::ArithmeticForClause(arith_for) => {
            let body_start = commands.len();
            extract_commands_from_do_group(&arith_for.body, commands)?;
            Some(body_start)
        }
        ast::CompoundCommand::Arithmetic(_arith_cmd) => Some(commands.len()),
    }
}

pub(super) fn extract_commands_from_do_group(
    do_group: &ast::DoGroupCommand,
    commands: &mut Vec<String>,
) -> Option<()> {
    extract_commands_from_compound_list(&do_group.list, commands)
}

pub(super) fn extract_commands_from_function_body(
    func_body: &ast::FunctionBody,
    commands: &mut Vec<String>,
) -> Option<()> {
    let body_start = extract_commands_from_compound_command(&func_body.0, commands)?;
    if let Some(redirect_list) = &func_body.1 {
        let mut normalized_redirects = Vec::new();
        for redirect in &redirect_list.0 {
            match normalize_io_redirect(redirect)? {
                RedirectNormalization::Normalized(s) => normalized_redirects.push(s),
                RedirectNormalization::Skip => {}
            }
        }
        if !normalized_redirects.is_empty() {
            if body_start >= commands.len() {
                return None;
            }
            commands.extend(normalized_redirects);
        }
        for redirect in &redirect_list.0 {
            extract_commands_from_io_redirect(redirect, commands)?;
        }
    }
    Some(())
}

pub(super) fn extract_commands_from_extended_test_expr(
    test_expr: &ast::ExtendedTestExprCommand,
    commands: &mut Vec<String>,
) -> Option<()> {
    extract_commands_from_extended_test_expr_inner(&test_expr.expr, commands)
}

pub(super) fn extract_commands_from_extended_test_expr_inner(
    expr: &ast::ExtendedTestExpr,
    commands: &mut Vec<String>,
) -> Option<()> {
    match expr {
        ast::ExtendedTestExpr::Not(inner) => {
            extract_commands_from_extended_test_expr_inner(inner, commands)?;
        }
        ast::ExtendedTestExpr::And(left, right) | ast::ExtendedTestExpr::Or(left, right) => {
            extract_commands_from_extended_test_expr_inner(left, commands)?;
            extract_commands_from_extended_test_expr_inner(right, commands)?;
        }
        ast::ExtendedTestExpr::Parenthesized(inner) => {
            extract_commands_from_extended_test_expr_inner(inner, commands)?;
        }
        ast::ExtendedTestExpr::UnaryTest(_, word) => {
            extract_commands_from_word(word, commands)?;
        }
        ast::ExtendedTestExpr::BinaryTest(_, word1, word2) => {
            extract_commands_from_word(word1, commands)?;
            extract_commands_from_word(word2, commands)?;
        }
    }
    Some(())
}

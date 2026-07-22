use super::*;

pub(super) enum TerminalProgramValidation {
    Safe,
    Unsafe,
    Unsupported,
}

pub(super) fn program_validation(program: &ast::Program) -> TerminalProgramValidation {
    combine_validations(
        program
            .complete_commands
            .iter()
            .map(compound_list_validation),
    )
}

fn compound_list_validation(compound_list: &ast::CompoundList) -> TerminalProgramValidation {
    combine_validations(
        compound_list
            .0
            .iter()
            .map(|item| and_or_list_validation(&item.0)),
    )
}

fn and_or_list_validation(and_or_list: &ast::AndOrList) -> TerminalProgramValidation {
    combine_validations(
        std::iter::once(pipeline_validation(&and_or_list.first)).chain(
            and_or_list.additional.iter().map(|and_or| match and_or {
                ast::AndOr::And(pipeline) | ast::AndOr::Or(pipeline) => {
                    pipeline_validation(pipeline)
                }
            }),
        ),
    )
}

fn pipeline_validation(pipeline: &ast::Pipeline) -> TerminalProgramValidation {
    combine_validations(pipeline.seq.iter().map(command_validation))
}

fn command_validation(command: &ast::Command) -> TerminalProgramValidation {
    match command {
        ast::Command::Simple(simple_command) => simple_command_validation(simple_command),
        ast::Command::Compound(compound_command, redirect_list) => combine_validations(
            std::iter::once(compound_command_validation(compound_command))
                .chain(redirect_list.iter().map(redirect_list_validation)),
        ),
        ast::Command::Function(function_definition) => {
            function_body_validation(&function_definition.body)
        }
        ast::Command::ExtendedTest(test_expr) => extended_test_expr_validation(test_expr),
    }
}

fn simple_command_validation(simple_command: &ast::SimpleCommand) -> TerminalProgramValidation {
    combine_validations(
        simple_command
            .prefix
            .iter()
            .map(command_prefix_validation)
            .chain(simple_command.word_or_name.iter().map(word_validation))
            .chain(simple_command.suffix.iter().map(command_suffix_validation)),
    )
}

fn command_prefix_validation(prefix: &ast::CommandPrefix) -> TerminalProgramValidation {
    combine_validations(prefix.0.iter().map(prefix_or_suffix_item_validation))
}

fn command_suffix_validation(suffix: &ast::CommandSuffix) -> TerminalProgramValidation {
    combine_validations(suffix.0.iter().map(prefix_or_suffix_item_validation))
}

fn prefix_or_suffix_item_validation(
    item: &ast::CommandPrefixOrSuffixItem,
) -> TerminalProgramValidation {
    match item {
        ast::CommandPrefixOrSuffixItem::IoRedirect(redirect) => io_redirect_validation(redirect),
        ast::CommandPrefixOrSuffixItem::Word(word) => word_validation(word),
        ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word) => {
            combine_validations([assignment_validation(assignment), word_validation(word)])
        }
        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, _) => {
            TerminalProgramValidation::Unsafe
        }
    }
}

fn io_redirect_validation(redirect: &ast::IoRedirect) -> TerminalProgramValidation {
    match redirect {
        ast::IoRedirect::File(_, _, target) => match target {
            ast::IoFileRedirectTarget::Filename(word) => word_validation(word),
            ast::IoFileRedirectTarget::ProcessSubstitution(_, _) => {
                TerminalProgramValidation::Unsafe
            }
            _ => TerminalProgramValidation::Safe,
        },
        ast::IoRedirect::HereDocument(_, here_doc) => {
            if here_doc.requires_expansion {
                word_validation(&here_doc.doc)
            } else {
                TerminalProgramValidation::Safe
            }
        }
        ast::IoRedirect::HereString(_, word) | ast::IoRedirect::OutputAndError(word, _) => {
            word_validation(word)
        }
    }
}

fn assignment_validation(assignment: &ast::Assignment) -> TerminalProgramValidation {
    match &assignment.value {
        ast::AssignmentValue::Scalar(word) => word_validation(word),
        ast::AssignmentValue::Array(words) => {
            combine_validations(words.iter().flat_map(|(key, value)| {
                key.iter()
                    .map(word_validation)
                    .chain(std::iter::once(word_validation(value)))
            }))
        }
    }
}

fn word_validation(word: &ast::Word) -> TerminalProgramValidation {
    let options = ParserOptions::default();
    let pieces = match brush_parser::word::parse(&word.value, &options) {
        Ok(pieces) => pieces,
        Err(_) => return TerminalProgramValidation::Unsupported,
    };

    combine_validations(
        pieces
            .iter()
            .map(|piece_with_source| word_piece_validation(&piece_with_source.piece)),
    )
}

fn word_piece_validation(piece: &WordPiece) -> TerminalProgramValidation {
    match piece {
        WordPiece::Text(_)
        | WordPiece::SingleQuotedText(_)
        | WordPiece::AnsiCQuotedText(_)
        | WordPiece::EscapeSequence(_)
        | WordPiece::TildePrefix(_) => TerminalProgramValidation::Safe,
        WordPiece::DoubleQuotedSequence(pieces)
        | WordPiece::GettextDoubleQuotedSequence(pieces) => combine_validations(
            pieces
                .iter()
                .map(|inner| word_piece_validation(&inner.piece)),
        ),
        WordPiece::ParameterExpansion(_) | WordPiece::ArithmeticExpression(_) => {
            TerminalProgramValidation::Unsafe
        }
        WordPiece::CommandSubstitution(command)
        | WordPiece::BackquotedCommandSubstitution(command) => {
            let reader = BufReader::new(command.as_bytes());
            let options = ParserOptions::default();
            let source_info = SourceInfo::default();
            let mut parser = Parser::new(reader, &options, &source_info);

            match parser.parse_program() {
                Ok(_) => TerminalProgramValidation::Unsafe,
                Err(_) => TerminalProgramValidation::Unsupported,
            }
        }
    }
}

fn compound_command_validation(
    compound_command: &ast::CompoundCommand,
) -> TerminalProgramValidation {
    match compound_command {
        ast::CompoundCommand::BraceGroup(brace_group) => {
            compound_list_validation(&brace_group.list)
        }
        ast::CompoundCommand::Subshell(subshell) => compound_list_validation(&subshell.list),
        ast::CompoundCommand::ForClause(for_clause) => combine_validations(
            for_clause
                .values
                .iter()
                .flat_map(|values| values.iter().map(word_validation))
                .chain(std::iter::once(do_group_validation(&for_clause.body))),
        ),
        ast::CompoundCommand::CaseClause(case_clause) => combine_validations(
            std::iter::once(word_validation(&case_clause.value))
                .chain(
                    case_clause
                        .cases
                        .iter()
                        .flat_map(|item| item.cmd.iter().map(compound_list_validation)),
                )
                .chain(
                    case_clause
                        .cases
                        .iter()
                        .flat_map(|item| item.patterns.iter().map(word_validation)),
                ),
        ),
        ast::CompoundCommand::IfClause(if_clause) => combine_validations(
            std::iter::once(compound_list_validation(&if_clause.condition))
                .chain(std::iter::once(compound_list_validation(&if_clause.then)))
                .chain(if_clause.elses.iter().flat_map(|elses| {
                    elses.iter().flat_map(|else_item| {
                        else_item
                            .condition
                            .iter()
                            .map(compound_list_validation)
                            .chain(std::iter::once(compound_list_validation(&else_item.body)))
                    })
                })),
        ),
        ast::CompoundCommand::WhileClause(while_clause)
        | ast::CompoundCommand::UntilClause(while_clause) => combine_validations([
            compound_list_validation(&while_clause.0),
            do_group_validation(&while_clause.1),
        ]),
        ast::CompoundCommand::ArithmeticForClause(_) => TerminalProgramValidation::Unsafe,
        ast::CompoundCommand::Arithmetic(_) => TerminalProgramValidation::Unsafe,
    }
}

fn do_group_validation(do_group: &ast::DoGroupCommand) -> TerminalProgramValidation {
    compound_list_validation(&do_group.list)
}

fn function_body_validation(function_body: &ast::FunctionBody) -> TerminalProgramValidation {
    combine_validations(
        std::iter::once(compound_command_validation(&function_body.0))
            .chain(function_body.1.iter().map(redirect_list_validation)),
    )
}

fn redirect_list_validation(redirect_list: &ast::RedirectList) -> TerminalProgramValidation {
    combine_validations(redirect_list.0.iter().map(io_redirect_validation))
}

fn extended_test_expr_validation(
    test_expr: &ast::ExtendedTestExprCommand,
) -> TerminalProgramValidation {
    extended_test_expr_inner_validation(&test_expr.expr)
}

fn extended_test_expr_inner_validation(expr: &ast::ExtendedTestExpr) -> TerminalProgramValidation {
    match expr {
        ast::ExtendedTestExpr::Not(inner) | ast::ExtendedTestExpr::Parenthesized(inner) => {
            extended_test_expr_inner_validation(inner)
        }
        ast::ExtendedTestExpr::And(left, right) | ast::ExtendedTestExpr::Or(left, right) => {
            combine_validations([
                extended_test_expr_inner_validation(left),
                extended_test_expr_inner_validation(right),
            ])
        }
        ast::ExtendedTestExpr::UnaryTest(_, word) => word_validation(word),
        ast::ExtendedTestExpr::BinaryTest(_, left, right) => {
            combine_validations([word_validation(left), word_validation(right)])
        }
    }
}

fn combine_validations(
    validations: impl IntoIterator<Item = TerminalProgramValidation>,
) -> TerminalProgramValidation {
    let mut saw_unsafe = false;
    let mut saw_unsupported = false;

    for validation in validations {
        match validation {
            TerminalProgramValidation::Unsupported => saw_unsupported = true,
            TerminalProgramValidation::Unsafe => saw_unsafe = true,
            TerminalProgramValidation::Safe => {}
        }
    }

    if saw_unsafe {
        TerminalProgramValidation::Unsafe
    } else if saw_unsupported {
        TerminalProgramValidation::Unsupported
    } else {
        TerminalProgramValidation::Safe
    }
}

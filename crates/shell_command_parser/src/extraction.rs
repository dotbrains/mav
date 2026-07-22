use super::*;

pub(super) fn extract_commands_from_program(
    program: &ast::Program,
    commands: &mut Vec<String>,
) -> Option<()> {
    for complete_command in &program.complete_commands {
        extract_commands_from_compound_list(complete_command, commands)?;
    }
    Some(())
}

pub(super) fn extract_commands_from_compound_list(
    compound_list: &ast::CompoundList,
    commands: &mut Vec<String>,
) -> Option<()> {
    for item in &compound_list.0 {
        extract_commands_from_and_or_list(&item.0, commands)?;
    }
    Some(())
}

pub(super) fn extract_commands_from_and_or_list(
    and_or_list: &ast::AndOrList,
    commands: &mut Vec<String>,
) -> Option<()> {
    extract_commands_from_pipeline(&and_or_list.first, commands)?;

    for and_or in &and_or_list.additional {
        match and_or {
            ast::AndOr::And(pipeline) | ast::AndOr::Or(pipeline) => {
                extract_commands_from_pipeline(pipeline, commands)?;
            }
        }
    }
    Some(())
}

pub(super) fn extract_commands_from_pipeline(
    pipeline: &ast::Pipeline,
    commands: &mut Vec<String>,
) -> Option<()> {
    for command in &pipeline.seq {
        extract_commands_from_command(command, commands)?;
    }
    Some(())
}

pub(super) fn extract_commands_from_command(
    command: &ast::Command,
    commands: &mut Vec<String>,
) -> Option<()> {
    match command {
        ast::Command::Simple(simple_command) => {
            extract_commands_from_simple_command(simple_command, commands)?;
        }
        ast::Command::Compound(compound_command, redirect_list) => {
            let body_start = extract_commands_from_compound_command(compound_command, commands)?;
            if let Some(redirect_list) = redirect_list {
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
        }
        ast::Command::Function(func_def) => {
            extract_commands_from_function_body(&func_def.body, commands)?;
        }
        ast::Command::ExtendedTest(test_expr) => {
            extract_commands_from_extended_test_expr(test_expr, commands)?;
        }
    }
    Some(())
}

pub(super) fn extract_commands_from_simple_command(
    simple_command: &ast::SimpleCommand,
    commands: &mut Vec<String>,
) -> Option<()> {
    // Build a normalized command string from individual words, stripping shell
    // quotes so that security patterns match regardless of quoting style.
    // For example, both `rm -rf '/'` and `rm -rf /` normalize to "rm -rf /".
    //
    // If any word fails to normalize, we return None so that `extract_commands`
    // returns None — the same as a shell parse failure. The caller then falls
    // back to raw-input matching with always_allow disabled.
    let mut words = Vec::new();
    let mut redirects = Vec::new();

    if let Some(prefix) = &simple_command.prefix {
        for item in &prefix.0 {
            match item {
                ast::CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                    match normalize_io_redirect(redirect) {
                        Some(RedirectNormalization::Normalized(s)) => redirects.push(s),
                        Some(RedirectNormalization::Skip) => {}
                        None => return None,
                    }
                }
                ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word) => {
                    match normalize_assignment_for_command_prefix(assignment, word)? {
                        NormalizedAssignment::Included(normalized_assignment) => {
                            words.push(normalized_assignment);
                        }
                        NormalizedAssignment::Skipped => {}
                    }
                }
                ast::CommandPrefixOrSuffixItem::Word(word) => {
                    words.push(normalize_word(word)?);
                }
                ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, _) => return None,
            }
        }
    }
    if let Some(word) = &simple_command.word_or_name {
        words.push(normalize_word(word)?);
    }
    if let Some(suffix) = &simple_command.suffix {
        for item in &suffix.0 {
            match item {
                ast::CommandPrefixOrSuffixItem::Word(word) => {
                    words.push(normalize_word(word)?);
                }
                ast::CommandPrefixOrSuffixItem::IoRedirect(redirect) => {
                    match normalize_io_redirect(redirect) {
                        Some(RedirectNormalization::Normalized(s)) => redirects.push(s),
                        Some(RedirectNormalization::Skip) => {}
                        None => return None,
                    }
                }
                ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word) => {
                    match normalize_assignment_for_command_prefix(assignment, word)? {
                        NormalizedAssignment::Included(normalized_assignment) => {
                            words.push(normalized_assignment);
                        }
                        NormalizedAssignment::Skipped => {}
                    }
                }
                ast::CommandPrefixOrSuffixItem::ProcessSubstitution(_, _) => {}
            }
        }
    }

    if words.is_empty() && !redirects.is_empty() {
        return None;
    }

    let command_str = words.join(" ");
    if !command_str.is_empty() {
        commands.push(command_str);
    }
    commands.extend(redirects);

    // Extract nested commands from command substitutions, process substitutions, etc.
    if let Some(prefix) = &simple_command.prefix {
        extract_commands_from_command_prefix(prefix, commands)?;
    }
    if let Some(word) = &simple_command.word_or_name {
        extract_commands_from_word(word, commands)?;
    }
    if let Some(suffix) = &simple_command.suffix {
        extract_commands_from_command_suffix(suffix, commands)?;
    }
    Some(())
}

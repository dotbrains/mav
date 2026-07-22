use brush_parser::ast;
use brush_parser::ast::SourceLocation;
use brush_parser::word::WordPiece;
use brush_parser::{Parser, ParserOptions, SourceInfo};
use std::io::BufReader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCommandPrefix {
    pub normalized: String,
    pub display: String,
    pub tokens: Vec<String>,
    pub command: String,
    pub subcommand: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalCommandValidation {
    Safe,
    Unsafe,
    Unsupported,
}

pub fn extract_commands(command: &str) -> Option<Vec<String>> {
    let reader = BufReader::new(command.as_bytes());
    let options = ParserOptions::default();
    let source_info = SourceInfo::default();
    let mut parser = Parser::new(reader, &options, &source_info);

    let program = parser.parse_program().ok()?;

    let mut commands = Vec::new();
    extract_commands_from_program(&program, &mut commands)?;

    Some(commands)
}

pub fn extract_terminal_command_prefix(command: &str) -> Option<TerminalCommandPrefix> {
    let reader = BufReader::new(command.as_bytes());
    let options = ParserOptions::default();
    let source_info = SourceInfo::default();
    let mut parser = Parser::new(reader, &options, &source_info);

    let program = parser.parse_program().ok()?;
    let simple_command = first_simple_command(&program)?;

    let mut normalized_tokens = Vec::new();
    let mut display_start = None;
    let mut display_end = None;

    if let Some(prefix) = &simple_command.prefix {
        for item in &prefix.0 {
            if let ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word) = item {
                match normalize_assignment_for_command_prefix(assignment, word)? {
                    NormalizedAssignment::Included(normalized_assignment) => {
                        normalized_tokens.push(normalized_assignment);
                        update_display_bounds(&mut display_start, &mut display_end, word);
                    }
                    NormalizedAssignment::Skipped => {}
                }
            }
        }
    }

    let command_word = simple_command.word_or_name.as_ref()?;
    let command_name = normalize_word(command_word)?;
    normalized_tokens.push(command_name.clone());
    update_display_bounds(&mut display_start, &mut display_end, command_word);

    let mut subcommand = None;
    if let Some(suffix) = &simple_command.suffix {
        for item in &suffix.0 {
            match item {
                ast::CommandPrefixOrSuffixItem::IoRedirect(_) => continue,
                ast::CommandPrefixOrSuffixItem::Word(word) => {
                    let normalized_word = normalize_word(word)?;
                    if !normalized_word.starts_with('-') {
                        subcommand = Some(normalized_word.clone());
                        normalized_tokens.push(normalized_word);
                        update_display_bounds(&mut display_start, &mut display_end, word);
                    }
                    break;
                }
                _ => break,
            }
        }
    }

    let start = display_start?;
    let end = display_end?;
    let display = command.get(start..end)?.to_string();

    Some(TerminalCommandPrefix {
        normalized: normalized_tokens.join(" "),
        display,
        tokens: normalized_tokens,
        command: command_name,
        subcommand,
    })
}

pub fn validate_terminal_command(command: &str) -> TerminalCommandValidation {
    let reader = BufReader::new(command.as_bytes());
    let options = ParserOptions::default();
    let source_info = SourceInfo::default();
    let mut parser = Parser::new(reader, &options, &source_info);

    let program = match parser.parse_program() {
        Ok(program) => program,
        Err(_) => return TerminalCommandValidation::Unsupported,
    };

    match program_validation(&program) {
        TerminalProgramValidation::Safe => TerminalCommandValidation::Safe,
        TerminalProgramValidation::Unsafe => TerminalCommandValidation::Unsafe,
        TerminalProgramValidation::Unsupported => TerminalCommandValidation::Unsupported,
    }
}

mod common;
mod extraction;
mod nested_extraction;
mod redirects;
#[cfg(test)]
mod tests;
mod validation;

use common::{
    NormalizedAssignment, first_simple_command, normalize_assignment_for_command_prefix,
    normalize_word, update_display_bounds,
};
use extraction::*;
use nested_extraction::*;
use redirects::{RedirectNormalization, normalize_io_redirect};
use validation::{TerminalProgramValidation, program_validation};

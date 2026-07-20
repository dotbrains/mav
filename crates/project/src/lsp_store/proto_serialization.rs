use std::{convert::TryInto, mem, path::Path};

use anyhow::{Context as _, Result, anyhow};
use language::{
    LanguageServerId, LanguageServerName, PointUtf16, Unclipped,
    proto::{deserialize_anchor, serialize_anchor},
};
use lsp::SymbolKind;
use rpc::proto;
use util::rel_path::RelPath;
use worktree::WorktreeId;

use crate::{
    CodeAction, CompletionSource, CoreCompletion, LspAction, ProjectPath, Symbol,
    lsp_store::{LspStore, SymbolLocation, symbol_types::CoreSymbol},
};

impl LspStore {
    pub(crate) fn serialize_symbol(symbol: &Symbol) -> proto::Symbol {
        let mut result = proto::Symbol {
            language_server_name: symbol.language_server_name.0.to_string(),
            source_worktree_id: symbol.source_worktree_id.to_proto(),
            language_server_id: symbol.source_language_server_id.to_proto(),
            name: symbol.name.clone(),
            kind: unsafe { mem::transmute::<SymbolKind, i32>(symbol.kind) },
            start: Some(proto::PointUtf16 {
                row: symbol.range.start.0.row,
                column: symbol.range.start.0.column,
            }),
            end: Some(proto::PointUtf16 {
                row: symbol.range.end.0.row,
                column: symbol.range.end.0.column,
            }),
            worktree_id: Default::default(),
            path: Default::default(),
            signature: Default::default(),
            container_name: symbol.container_name.clone(),
        };
        match &symbol.path {
            SymbolLocation::InProject(path) => {
                result.worktree_id = path.worktree_id.to_proto();
                result.path = path.path.to_proto();
            }
            SymbolLocation::OutsideProject {
                abs_path,
                signature,
            } => {
                result.path = abs_path.to_string_lossy().into_owned();
                result.signature = signature.to_vec();
            }
        }
        result
    }

    pub(crate) fn deserialize_symbol(serialized_symbol: proto::Symbol) -> Result<CoreSymbol> {
        let source_worktree_id = WorktreeId::from_proto(serialized_symbol.source_worktree_id);
        let worktree_id = WorktreeId::from_proto(serialized_symbol.worktree_id);
        let kind = unsafe { mem::transmute::<i32, SymbolKind>(serialized_symbol.kind) };

        let path = if serialized_symbol.signature.is_empty() {
            SymbolLocation::InProject(ProjectPath {
                worktree_id,
                path: RelPath::from_proto(&serialized_symbol.path)
                    .context("invalid symbol path")?,
            })
        } else {
            SymbolLocation::OutsideProject {
                abs_path: Path::new(&serialized_symbol.path).into(),
                signature: serialized_symbol
                    .signature
                    .try_into()
                    .map_err(|_| anyhow!("invalid signature"))?,
            }
        };

        let start = serialized_symbol.start.context("invalid start")?;
        let end = serialized_symbol.end.context("invalid end")?;
        Ok(CoreSymbol {
            language_server_name: LanguageServerName(serialized_symbol.language_server_name.into()),
            source_worktree_id,
            source_language_server_id: LanguageServerId::from_proto(
                serialized_symbol.language_server_id,
            ),
            path,
            name: serialized_symbol.name,
            range: Unclipped(PointUtf16::new(start.row, start.column))
                ..Unclipped(PointUtf16::new(end.row, end.column)),
            kind,
            container_name: serialized_symbol.container_name,
        })
    }

    pub(crate) fn serialize_completion(completion: &CoreCompletion) -> proto::Completion {
        let mut serialized_completion = proto::Completion {
            old_replace_start: Some(serialize_anchor(&completion.replace_range.start)),
            old_replace_end: Some(serialize_anchor(&completion.replace_range.end)),
            new_text: completion.new_text.clone(),
            ..proto::Completion::default()
        };
        match &completion.source {
            CompletionSource::Lsp {
                insert_range,
                server_id,
                lsp_completion,
                lsp_defaults,
                resolved,
            } => {
                let (old_insert_start, old_insert_end) = insert_range
                    .as_ref()
                    .map(|range| (serialize_anchor(&range.start), serialize_anchor(&range.end)))
                    .unzip();

                serialized_completion.old_insert_start = old_insert_start;
                serialized_completion.old_insert_end = old_insert_end;
                serialized_completion.source = proto::completion::Source::Lsp as i32;
                serialized_completion.server_id = server_id.0 as u64;
                serialized_completion.lsp_completion = serde_json::to_vec(lsp_completion).unwrap();
                serialized_completion.lsp_defaults = lsp_defaults
                    .as_deref()
                    .map(|lsp_defaults| serde_json::to_vec(lsp_defaults).unwrap());
                serialized_completion.resolved = *resolved;
            }
            CompletionSource::BufferWord {
                word_range,
                resolved,
            } => {
                serialized_completion.source = proto::completion::Source::BufferWord as i32;
                serialized_completion.buffer_word_start = Some(serialize_anchor(&word_range.start));
                serialized_completion.buffer_word_end = Some(serialize_anchor(&word_range.end));
                serialized_completion.resolved = *resolved;
            }
            CompletionSource::Custom => {
                serialized_completion.source = proto::completion::Source::Custom as i32;
                serialized_completion.resolved = true;
            }
            CompletionSource::Dap { sort_text } => {
                serialized_completion.source = proto::completion::Source::Dap as i32;
                serialized_completion.sort_text = Some(sort_text.clone());
            }
        }

        serialized_completion
    }

    pub(crate) fn deserialize_completion(completion: proto::Completion) -> Result<CoreCompletion> {
        let old_replace_start = completion
            .old_replace_start
            .and_then(deserialize_anchor)
            .context("invalid old start")?;
        let old_replace_end = completion
            .old_replace_end
            .and_then(deserialize_anchor)
            .context("invalid old end")?;
        let insert_range = match completion.old_insert_start.zip(completion.old_insert_end) {
            Some((start, end)) => {
                let start = deserialize_anchor(start).context("invalid insert old start")?;
                let end = deserialize_anchor(end).context("invalid insert old end")?;
                Some(start..end)
            }
            None => None,
        };
        Ok(CoreCompletion {
            replace_range: old_replace_start..old_replace_end,
            new_text: completion.new_text,
            source: match proto::completion::Source::from_i32(completion.source) {
                Some(proto::completion::Source::Custom) => CompletionSource::Custom,
                Some(proto::completion::Source::Lsp) => CompletionSource::Lsp {
                    insert_range,
                    server_id: LanguageServerId::from_proto(completion.server_id),
                    lsp_completion: serde_json::from_slice(&completion.lsp_completion)?,
                    lsp_defaults: completion
                        .lsp_defaults
                        .as_deref()
                        .map(serde_json::from_slice)
                        .transpose()?,
                    resolved: completion.resolved,
                },
                Some(proto::completion::Source::BufferWord) => {
                    let word_range = completion
                        .buffer_word_start
                        .and_then(deserialize_anchor)
                        .context("invalid buffer word start")?
                        ..completion
                            .buffer_word_end
                            .and_then(deserialize_anchor)
                            .context("invalid buffer word end")?;
                    CompletionSource::BufferWord {
                        word_range,
                        resolved: completion.resolved,
                    }
                }
                Some(proto::completion::Source::Dap) => CompletionSource::Dap {
                    sort_text: completion
                        .sort_text
                        .context("expected sort text to exist")?,
                },
                _ => anyhow::bail!("Unexpected completion source {}", completion.source),
            },
        })
    }

    pub(crate) fn serialize_code_action(action: &CodeAction) -> proto::CodeAction {
        let (kind, lsp_action) = match &action.lsp_action {
            LspAction::Action(code_action) => (
                proto::code_action::Kind::Action as i32,
                serde_json::to_vec(code_action).unwrap(),
            ),
            LspAction::Command(command) => (
                proto::code_action::Kind::Command as i32,
                serde_json::to_vec(command).unwrap(),
            ),
            LspAction::CodeLens(code_lens) => (
                proto::code_action::Kind::CodeLens as i32,
                serde_json::to_vec(code_lens).unwrap(),
            ),
        };

        proto::CodeAction {
            server_id: action.server_id.0 as u64,
            start: Some(serialize_anchor(&action.range.start)),
            end: Some(serialize_anchor(&action.range.end)),
            lsp_action,
            kind,
            resolved: action.resolved,
        }
    }

    pub(crate) fn deserialize_code_action(action: proto::CodeAction) -> Result<CodeAction> {
        let start = action
            .start
            .and_then(deserialize_anchor)
            .context("invalid start")?;
        let end = action
            .end
            .and_then(deserialize_anchor)
            .context("invalid end")?;
        let lsp_action = match proto::code_action::Kind::from_i32(action.kind) {
            Some(proto::code_action::Kind::Action) => {
                LspAction::Action(serde_json::from_slice(&action.lsp_action)?)
            }
            Some(proto::code_action::Kind::Command) => {
                LspAction::Command(serde_json::from_slice(&action.lsp_action)?)
            }
            Some(proto::code_action::Kind::CodeLens) => {
                LspAction::CodeLens(serde_json::from_slice(&action.lsp_action)?)
            }
            None => anyhow::bail!("Unknown action kind {}", action.kind),
        };
        Ok(CodeAction {
            server_id: LanguageServerId(action.server_id as usize),
            range: start..end,
            resolved: action.resolved,
            lsp_action,
        })
    }
}

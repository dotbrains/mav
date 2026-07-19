//! Handles conversions of `language` items to and from the [`rpc`] protocol.

use crate::{CursorShape, Diagnostic, DiagnosticSourceKind, diagnostic_set::DiagnosticEntry};
use anyhow::{Context as _, Result};
use clock::ReplicaId;
use gpui::SharedString;
use lsp::{DiagnosticSeverity, LanguageServerId};
use rpc::proto;
use serde_json::Value;
use std::{ops::Range, str::FromStr, sync::Arc};
use text::*;

pub use proto::{BufferState, File, Operation};
pub use serialize::*;

use super::{point_from_lsp, point_to_lsp};

#[path = "proto/serialize.rs"]
mod serialize;

/// Deserializes a `[text::LineEnding]` from the RPC representation.
pub fn deserialize_line_ending(message: proto::LineEnding) -> text::LineEnding {
    match message {
        proto::LineEnding::Unix => text::LineEnding::Unix,
        proto::LineEnding::Windows => text::LineEnding::Windows,
    }
}

/// Splits the given list of operations into chunks.
pub fn split_operations(
    mut operations: Vec<proto::Operation>,
) -> impl Iterator<Item = Vec<proto::Operation>> {
    #[cfg(any(test, feature = "test-support"))]
    const CHUNK_SIZE: usize = 5;

    #[cfg(not(any(test, feature = "test-support")))]
    const CHUNK_SIZE: usize = 100;

    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }

        let operations = operations
            .drain(..std::cmp::min(CHUNK_SIZE, operations.len()))
            .collect::<Vec<_>>();
        if operations.is_empty() {
            done = true;
        }
        Some(operations)
    })
}

/// Deserializes a [`CursorShape`] from the RPC representation.
pub fn deserialize_cursor_shape(cursor_shape: proto::CursorShape) -> CursorShape {
    match cursor_shape {
        proto::CursorShape::CursorBar => CursorShape::Bar,
        proto::CursorShape::CursorBlock => CursorShape::Block,
        proto::CursorShape::CursorUnderscore => CursorShape::Underline,
        proto::CursorShape::CursorHollow => CursorShape::Hollow,
    }
}

/// Deserializes an [`Range<Anchor>`] from the RPC representation.
pub fn deserialize_anchor_range(range: proto::AnchorRange) -> Result<Range<Anchor>> {
    Ok(
        deserialize_anchor(range.start.context("invalid anchor")?).context("invalid anchor")?
            ..deserialize_anchor(range.end.context("invalid anchor")?).context("invalid anchor")?,
    )
}

// This behavior is currently copied in the collab database, for snapshotting channel notes
/// Deserializes an [`crate::Operation`] from the RPC representation.
pub fn deserialize_operation(message: proto::Operation) -> Result<crate::Operation> {
    Ok(
        match message.variant.context("missing operation variant")? {
            proto::operation::Variant::Edit(edit) => {
                crate::Operation::Buffer(text::Operation::Edit(deserialize_edit_operation(edit)))
            }
            proto::operation::Variant::Undo(undo) => {
                crate::Operation::Buffer(text::Operation::Undo(UndoOperation {
                    timestamp: clock::Lamport {
                        replica_id: ReplicaId::new(undo.replica_id as u16),
                        value: undo.lamport_timestamp,
                    },
                    version: deserialize_version(&undo.version),
                    counts: undo
                        .counts
                        .into_iter()
                        .map(|c| {
                            (
                                clock::Lamport {
                                    replica_id: ReplicaId::new(c.replica_id as u16),
                                    value: c.lamport_timestamp,
                                },
                                c.count,
                            )
                        })
                        .collect(),
                }))
            }
            proto::operation::Variant::UpdateSelections(message) => {
                let selections = message
                    .selections
                    .into_iter()
                    .filter_map(|selection| {
                        Some(Selection {
                            id: selection.id as usize,
                            start: deserialize_anchor(selection.start?.anchor?)?,
                            end: deserialize_anchor(selection.end?.anchor?)?,
                            reversed: selection.reversed,
                            goal: SelectionGoal::None,
                        })
                    })
                    .collect::<Vec<_>>();

                crate::Operation::UpdateSelections {
                    lamport_timestamp: clock::Lamport {
                        replica_id: ReplicaId::new(message.replica_id as u16),
                        value: message.lamport_timestamp,
                    },
                    selections: Arc::from(selections),
                    line_mode: message.line_mode,
                    cursor_shape: deserialize_cursor_shape(
                        proto::CursorShape::from_i32(message.cursor_shape)
                            .context("Missing cursor shape")?,
                    ),
                }
            }
            proto::operation::Variant::UpdateDiagnostics(message) => {
                crate::Operation::UpdateDiagnostics {
                    lamport_timestamp: clock::Lamport {
                        replica_id: ReplicaId::new(message.replica_id as u16),
                        value: message.lamport_timestamp,
                    },
                    server_id: LanguageServerId(message.server_id as usize),
                    diagnostics: deserialize_diagnostics(message.diagnostics),
                }
            }
            proto::operation::Variant::UpdateCompletionTriggers(message) => {
                crate::Operation::UpdateCompletionTriggers {
                    triggers: message.triggers,
                    lamport_timestamp: clock::Lamport {
                        replica_id: ReplicaId::new(message.replica_id as u16),
                        value: message.lamport_timestamp,
                    },
                    server_id: LanguageServerId::from_proto(message.language_server_id),
                }
            }
            proto::operation::Variant::UpdateLineEnding(message) => {
                crate::Operation::UpdateLineEnding {
                    lamport_timestamp: clock::Lamport {
                        replica_id: ReplicaId::new(message.replica_id as u16),
                        value: message.lamport_timestamp,
                    },
                    line_ending: deserialize_line_ending(
                        proto::LineEnding::from_i32(message.line_ending)
                            .context("missing line_ending")?,
                    ),
                }
            }
        },
    )
}

/// Deserializes an [`EditOperation`] from the RPC representation.
pub fn deserialize_edit_operation(edit: proto::operation::Edit) -> EditOperation {
    EditOperation {
        timestamp: clock::Lamport {
            replica_id: ReplicaId::new(edit.replica_id as u16),
            value: edit.lamport_timestamp,
        },
        version: deserialize_version(&edit.version),
        ranges: edit.ranges.into_iter().map(deserialize_range).collect(),
        new_text: edit.new_text.into_iter().map(Arc::from).collect(),
    }
}

/// Deserializes an entry in the undo map from the RPC representation.
pub fn deserialize_undo_map_entry(
    entry: proto::UndoMapEntry,
) -> (clock::Lamport, Vec<(clock::Lamport, u32)>) {
    (
        clock::Lamport {
            replica_id: ReplicaId::new(entry.replica_id as u16),
            value: entry.local_timestamp,
        },
        entry
            .counts
            .into_iter()
            .map(|undo_count| {
                (
                    clock::Lamport {
                        replica_id: ReplicaId::new(undo_count.replica_id as u16),
                        value: undo_count.lamport_timestamp,
                    },
                    undo_count.count,
                )
            })
            .collect(),
    )
}

/// Deserializes selections from the RPC representation.
pub fn deserialize_selections(selections: Vec<proto::Selection>) -> Arc<[Selection<Anchor>]> {
    selections
        .into_iter()
        .filter_map(deserialize_selection)
        .collect()
}

/// Deserializes a [`Selection`] from the RPC representation.
pub fn deserialize_selection(selection: proto::Selection) -> Option<Selection<Anchor>> {
    Some(Selection {
        id: selection.id as usize,
        start: deserialize_anchor(selection.start?.anchor?)?,
        end: deserialize_anchor(selection.end?.anchor?)?,
        reversed: selection.reversed,
        goal: SelectionGoal::None,
    })
}

/// Deserializes a list of diagnostics from the RPC representation.
pub fn deserialize_diagnostics(
    diagnostics: Vec<proto::Diagnostic>,
) -> Arc<[DiagnosticEntry<Anchor>]> {
    diagnostics
        .into_iter()
        .filter_map(|diagnostic| {
            let data = if let Some(data) = diagnostic.data {
                Some(Value::from_str(&data).ok()?)
            } else {
                None
            };
            Some(DiagnosticEntry {
                range: deserialize_anchor(diagnostic.start?)?..deserialize_anchor(diagnostic.end?)?,
                diagnostic: Diagnostic {
                    source: diagnostic.source,
                    severity: match proto::diagnostic::Severity::from_i32(diagnostic.severity)? {
                        proto::diagnostic::Severity::Error => DiagnosticSeverity::ERROR,
                        proto::diagnostic::Severity::Warning => DiagnosticSeverity::WARNING,
                        proto::diagnostic::Severity::Information => DiagnosticSeverity::INFORMATION,
                        proto::diagnostic::Severity::Hint => DiagnosticSeverity::HINT,
                        proto::diagnostic::Severity::None => return None,
                    },
                    message: diagnostic.message,
                    markdown: diagnostic.markdown,
                    group_id: diagnostic.group_id as usize,
                    code: diagnostic.code.map(lsp::NumberOrString::from_string),
                    code_description: diagnostic
                        .code_description
                        .and_then(|s| lsp::Uri::from_str(&s).ok()),
                    is_primary: diagnostic.is_primary,
                    is_disk_based: diagnostic.is_disk_based,
                    is_unnecessary: diagnostic.is_unnecessary,
                    underline: diagnostic.underline,
                    registration_id: diagnostic.registration_id.map(SharedString::from),
                    source_kind: match proto::diagnostic::SourceKind::from_i32(
                        diagnostic.source_kind,
                    )? {
                        proto::diagnostic::SourceKind::Pulled => DiagnosticSourceKind::Pulled,
                        proto::diagnostic::SourceKind::Pushed => DiagnosticSourceKind::Pushed,
                        proto::diagnostic::SourceKind::Other => DiagnosticSourceKind::Other,
                    },
                    data,
                },
            })
        })
        .collect()
}

/// Deserializes an [`Anchor`] from the RPC representation.
pub fn deserialize_anchor(anchor: proto::Anchor) -> Option<Anchor> {
    let buffer_id = if let Some(id) = anchor.buffer_id {
        Some(BufferId::new(id).ok()?)
    } else {
        None
    };
    let timestamp = clock::Lamport {
        replica_id: ReplicaId::new(anchor.replica_id as u16),
        value: anchor.timestamp,
    };
    let bias = match proto::Bias::from_i32(anchor.bias)? {
        proto::Bias::Left => Bias::Left,
        proto::Bias::Right => Bias::Right,
    };
    Some(Anchor::new(
        timestamp,
        anchor.offset as u32,
        bias,
        buffer_id?,
    ))
}

/// Returns a `[clock::Lamport`] timestamp for the given [`proto::Operation`].
pub fn lamport_timestamp_for_operation(operation: &proto::Operation) -> Option<clock::Lamport> {
    let replica_id;
    let value;
    match operation.variant.as_ref()? {
        proto::operation::Variant::Edit(op) => {
            replica_id = op.replica_id;
            value = op.lamport_timestamp;
        }
        proto::operation::Variant::Undo(op) => {
            replica_id = op.replica_id;
            value = op.lamport_timestamp;
        }
        proto::operation::Variant::UpdateDiagnostics(op) => {
            replica_id = op.replica_id;
            value = op.lamport_timestamp;
        }
        proto::operation::Variant::UpdateSelections(op) => {
            replica_id = op.replica_id;
            value = op.lamport_timestamp;
        }
        proto::operation::Variant::UpdateCompletionTriggers(op) => {
            replica_id = op.replica_id;
            value = op.lamport_timestamp;
        }
        proto::operation::Variant::UpdateLineEnding(op) => {
            replica_id = op.replica_id;
            value = op.lamport_timestamp;
        }
    }

    Some(clock::Lamport {
        replica_id: ReplicaId::new(replica_id as u16),
        value,
    })
}

/// Deserializes a [`Transaction`] from the RPC representation.
pub fn deserialize_transaction(transaction: proto::Transaction) -> Result<Transaction> {
    Ok(Transaction {
        id: deserialize_timestamp(transaction.id.context("missing transaction id")?),
        edit_ids: transaction
            .edit_ids
            .into_iter()
            .map(deserialize_timestamp)
            .collect(),
        start: deserialize_version(&transaction.start),
    })
}

/// Deserializes a [`clock::Lamport`] timestamp from the RPC representation.
pub fn deserialize_timestamp(timestamp: proto::LamportTimestamp) -> clock::Lamport {
    clock::Lamport {
        replica_id: ReplicaId::new(timestamp.replica_id as u16),
        value: timestamp.value,
    }
}

/// Deserializes a range of [`FullOffset`]s from the RPC representation.
pub fn deserialize_range(range: proto::Range) -> Range<FullOffset> {
    FullOffset(range.start as usize)..FullOffset(range.end as usize)
}

/// Deserializes a clock version from the RPC representation.
pub fn deserialize_version(message: &[proto::VectorClockEntry]) -> clock::Global {
    let mut version = clock::Global::new();
    for entry in message {
        version.observe(clock::Lamport {
            replica_id: ReplicaId::new(entry.replica_id as u16),
            value: entry.timestamp,
        });
    }
    version
}

pub fn deserialize_lsp_edit(edit: proto::TextEdit) -> Option<lsp::TextEdit> {
    let start = edit.lsp_range_start?;
    let start = PointUtf16::new(start.row, start.column);
    let end = edit.lsp_range_end?;
    let end = PointUtf16::new(end.row, end.column);
    Some(lsp::TextEdit {
        range: lsp::Range {
            start: point_to_lsp(start),
            end: point_to_lsp(end),
        },
        new_text: edit.new_text,
    })
}

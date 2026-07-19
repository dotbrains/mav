use crate::{CursorShape, DiagnosticSourceKind, diagnostic_set::DiagnosticEntry};
use lsp::DiagnosticSeverity;
use rpc::proto;
use std::{ops::Range, sync::Arc};
use text::*;

use super::point_from_lsp;

/// Serializes a [`text::LineEnding`] to be sent over RPC.
pub fn serialize_line_ending(message: text::LineEnding) -> proto::LineEnding {
    match message {
        text::LineEnding::Unix => proto::LineEnding::Unix,
        text::LineEnding::Windows => proto::LineEnding::Windows,
    }
}

/// Serializes a [`crate::Operation`] to be sent over RPC.
pub fn serialize_operation(operation: &crate::Operation) -> proto::Operation {
    proto::Operation {
        variant: Some(match operation {
            crate::Operation::Buffer(text::Operation::Edit(edit)) => {
                proto::operation::Variant::Edit(serialize_edit_operation(edit))
            }

            crate::Operation::Buffer(text::Operation::Undo(undo)) => {
                proto::operation::Variant::Undo(proto::operation::Undo {
                    replica_id: undo.timestamp.replica_id.as_u16() as u32,
                    lamport_timestamp: undo.timestamp.value,
                    version: serialize_version(&undo.version),
                    counts: undo
                        .counts
                        .iter()
                        .map(|(edit_id, count)| proto::UndoCount {
                            replica_id: edit_id.replica_id.as_u16() as u32,
                            lamport_timestamp: edit_id.value,
                            count: *count,
                        })
                        .collect(),
                })
            }

            crate::Operation::UpdateSelections {
                selections,
                line_mode,
                lamport_timestamp,
                cursor_shape,
            } => proto::operation::Variant::UpdateSelections(proto::operation::UpdateSelections {
                replica_id: lamport_timestamp.replica_id.as_u16() as u32,
                lamport_timestamp: lamport_timestamp.value,
                selections: serialize_selections(selections),
                line_mode: *line_mode,
                cursor_shape: serialize_cursor_shape(cursor_shape) as i32,
            }),

            crate::Operation::UpdateDiagnostics {
                lamport_timestamp,
                server_id,
                diagnostics,
            } => proto::operation::Variant::UpdateDiagnostics(proto::UpdateDiagnostics {
                replica_id: lamport_timestamp.replica_id.as_u16() as u32,
                lamport_timestamp: lamport_timestamp.value,
                server_id: server_id.0 as u64,
                diagnostics: serialize_diagnostics(diagnostics.iter()),
            }),

            crate::Operation::UpdateCompletionTriggers {
                triggers,
                lamport_timestamp,
                server_id,
            } => proto::operation::Variant::UpdateCompletionTriggers(
                proto::operation::UpdateCompletionTriggers {
                    replica_id: lamport_timestamp.replica_id.as_u16() as u32,
                    lamport_timestamp: lamport_timestamp.value,
                    triggers: triggers.clone(),
                    language_server_id: server_id.to_proto(),
                },
            ),

            crate::Operation::UpdateLineEnding {
                line_ending,
                lamport_timestamp,
            } => proto::operation::Variant::UpdateLineEnding(proto::operation::UpdateLineEnding {
                replica_id: lamport_timestamp.replica_id.as_u16() as u32,
                lamport_timestamp: lamport_timestamp.value,
                line_ending: serialize_line_ending(*line_ending) as i32,
            }),
        }),
    }
}

/// Serializes an [`EditOperation`] to be sent over RPC.
pub fn serialize_edit_operation(operation: &EditOperation) -> proto::operation::Edit {
    proto::operation::Edit {
        replica_id: operation.timestamp.replica_id.as_u16() as u32,
        lamport_timestamp: operation.timestamp.value,
        version: serialize_version(&operation.version),
        ranges: operation.ranges.iter().map(serialize_range).collect(),
        new_text: operation
            .new_text
            .iter()
            .map(|text| text.to_string())
            .collect(),
    }
}

/// Serializes an entry in the undo map to be sent over RPC.
pub fn serialize_undo_map_entry(
    (edit_id, counts): (&clock::Lamport, &[(clock::Lamport, u32)]),
) -> proto::UndoMapEntry {
    proto::UndoMapEntry {
        replica_id: edit_id.replica_id.as_u16() as u32,
        local_timestamp: edit_id.value,
        counts: counts
            .iter()
            .map(|(undo_id, count)| proto::UndoCount {
                replica_id: undo_id.replica_id.as_u16() as u32,
                lamport_timestamp: undo_id.value,
                count: *count,
            })
            .collect(),
    }
}

/// Serializes selections to be sent over RPC.
pub fn serialize_selections(selections: &Arc<[Selection<Anchor>]>) -> Vec<proto::Selection> {
    selections.iter().map(serialize_selection).collect()
}

/// Serializes a [`Selection`] to be sent over RPC.
pub fn serialize_selection(selection: &Selection<Anchor>) -> proto::Selection {
    proto::Selection {
        id: selection.id as u64,
        start: Some(proto::EditorAnchor {
            anchor: Some(serialize_anchor(&selection.start)),
            excerpt_id: None,
        }),
        end: Some(proto::EditorAnchor {
            anchor: Some(serialize_anchor(&selection.end)),
            excerpt_id: None,
        }),
        reversed: selection.reversed,
    }
}

/// Serializes a [`CursorShape`] to be sent over RPC.
pub fn serialize_cursor_shape(cursor_shape: &CursorShape) -> proto::CursorShape {
    match cursor_shape {
        CursorShape::Bar => proto::CursorShape::CursorBar,
        CursorShape::Block => proto::CursorShape::CursorBlock,
        CursorShape::Underline => proto::CursorShape::CursorUnderscore,
        CursorShape::Hollow => proto::CursorShape::CursorHollow,
    }
}

/// Serializes a list of diagnostics to be sent over RPC.
pub fn serialize_diagnostics<'a>(
    diagnostics: impl IntoIterator<Item = &'a DiagnosticEntry<Anchor>>,
) -> Vec<proto::Diagnostic> {
    diagnostics
        .into_iter()
        .map(|entry| proto::Diagnostic {
            source: entry.diagnostic.source.clone(),
            source_kind: match entry.diagnostic.source_kind {
                DiagnosticSourceKind::Pulled => proto::diagnostic::SourceKind::Pulled,
                DiagnosticSourceKind::Pushed => proto::diagnostic::SourceKind::Pushed,
                DiagnosticSourceKind::Other => proto::diagnostic::SourceKind::Other,
            } as i32,
            start: Some(serialize_anchor(&entry.range.start)),
            end: Some(serialize_anchor(&entry.range.end)),
            message: entry.diagnostic.message.clone(),
            markdown: entry.diagnostic.markdown.clone(),
            severity: match entry.diagnostic.severity {
                DiagnosticSeverity::ERROR => proto::diagnostic::Severity::Error,
                DiagnosticSeverity::WARNING => proto::diagnostic::Severity::Warning,
                DiagnosticSeverity::INFORMATION => proto::diagnostic::Severity::Information,
                DiagnosticSeverity::HINT => proto::diagnostic::Severity::Hint,
                _ => proto::diagnostic::Severity::None,
            } as i32,
            group_id: entry.diagnostic.group_id as u64,
            is_primary: entry.diagnostic.is_primary,
            underline: entry.diagnostic.underline,
            code: entry.diagnostic.code.as_ref().map(|s| s.to_string()),
            code_description: entry
                .diagnostic
                .code_description
                .as_ref()
                .map(|s| s.to_string()),
            is_disk_based: entry.diagnostic.is_disk_based,
            is_unnecessary: entry.diagnostic.is_unnecessary,
            data: entry.diagnostic.data.as_ref().map(|data| data.to_string()),
            registration_id: entry
                .diagnostic
                .registration_id
                .as_ref()
                .map(ToString::to_string),
        })
        .collect()
}

/// Serializes an [`Anchor`] to be sent over RPC.
pub fn serialize_anchor(anchor: &Anchor) -> proto::Anchor {
    let timestamp = anchor.timestamp();
    proto::Anchor {
        replica_id: timestamp.replica_id.as_u16() as u32,
        timestamp: timestamp.value,
        offset: anchor.offset as u64,
        bias: match anchor.bias {
            Bias::Left => proto::Bias::Left as i32,
            Bias::Right => proto::Bias::Right as i32,
        },
        buffer_id: Some(anchor.buffer_id.into()),
    }
}

pub fn serialize_anchor_range(range: Range<Anchor>) -> proto::AnchorRange {
    proto::AnchorRange {
        start: Some(serialize_anchor(&range.start)),
        end: Some(serialize_anchor(&range.end)),
    }
}

/// Serializes a [`Transaction`] to be sent over RPC.
pub fn serialize_transaction(transaction: &Transaction) -> proto::Transaction {
    proto::Transaction {
        id: Some(serialize_timestamp(transaction.id)),
        edit_ids: transaction
            .edit_ids
            .iter()
            .copied()
            .map(serialize_timestamp)
            .collect(),
        start: serialize_version(&transaction.start),
    }
}

/// Serializes a [`clock::Lamport`] timestamp to be sent over RPC.
pub fn serialize_timestamp(timestamp: clock::Lamport) -> proto::LamportTimestamp {
    proto::LamportTimestamp {
        replica_id: timestamp.replica_id.as_u16() as u32,
        value: timestamp.value,
    }
}

/// Serializes a range of [`FullOffset`]s to be sent over RPC.
pub fn serialize_range(range: &Range<FullOffset>) -> proto::Range {
    proto::Range {
        start: range.start.0 as u64,
        end: range.end.0 as u64,
    }
}

/// Serializes a clock version to be sent over RPC.
pub fn serialize_version(version: &clock::Global) -> Vec<proto::VectorClockEntry> {
    version
        .iter()
        .map(|entry| proto::VectorClockEntry {
            replica_id: entry.replica_id.as_u16() as u32,
            timestamp: entry.value,
        })
        .collect()
}

pub fn serialize_lsp_edit(edit: lsp::TextEdit) -> proto::TextEdit {
    let start = point_from_lsp(edit.range.start).0;
    let end = point_from_lsp(edit.range.end).0;
    proto::TextEdit {
        new_text: edit.new_text,
        lsp_range_start: Some(proto::PointUtf16 {
            row: start.row,
            column: start.column,
        }),
        lsp_range_end: Some(proto::PointUtf16 {
            row: end.row,
            column: end.column,
        }),
    }
}

use super::*;

/// An operation used to synchronize this buffer with its other replicas.
#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    /// A text operation.
    Buffer(text::Operation),

    /// An update to the buffer's diagnostics.
    UpdateDiagnostics {
        /// The id of the language server that produced the new diagnostics.
        server_id: LanguageServerId,
        /// The diagnostics.
        diagnostics: Arc<[DiagnosticEntry<Anchor>]>,
        /// The buffer's lamport timestamp.
        lamport_timestamp: clock::Lamport,
    },

    /// An update to the most recent selections in this buffer.
    UpdateSelections {
        /// The selections.
        selections: Arc<[Selection<Anchor>]>,
        /// The buffer's lamport timestamp.
        lamport_timestamp: clock::Lamport,
        /// Whether the selections are in 'line mode'.
        line_mode: bool,
        /// The [`CursorShape`] associated with these selections.
        cursor_shape: CursorShape,
    },

    /// An update to the characters that should trigger autocompletion
    /// for this buffer.
    UpdateCompletionTriggers {
        /// The characters that trigger autocompletion.
        triggers: Vec<String>,
        /// The buffer's lamport timestamp.
        lamport_timestamp: clock::Lamport,
        /// The language server ID.
        server_id: LanguageServerId,
    },

    /// An update to the line ending type of this buffer.
    UpdateLineEnding {
        /// The line ending type.
        line_ending: LineEnding,
        /// The buffer's lamport timestamp.
        lamport_timestamp: clock::Lamport,
    },
}

/// An event that occurs in a buffer.
#[derive(Clone, Debug, PartialEq)]
pub enum BufferEvent {
    /// The buffer was changed in a way that must be
    /// propagated to its other replicas.
    Operation {
        operation: Operation,
        is_local: bool,
    },
    /// The buffer was edited.
    Edited { source: BufferEditSource },
    /// The buffer's `dirty` bit changed.
    DirtyChanged,
    /// The buffer was saved.
    Saved,
    /// The buffer's file was changed on disk.
    FileHandleChanged,
    /// The buffer was reloaded.
    Reloaded,
    /// The buffer is in need of a reload
    ReloadNeeded,
    /// The buffer's language was changed.
    /// The boolean indicates whether this buffer did not have a language before, but does now.
    LanguageChanged(bool),
    /// The buffer's syntax trees were updated.
    Reparsed,
    /// The buffer's diagnostics were updated.
    DiagnosticsUpdated,
    /// The buffer gained or lost editing capabilities.
    CapabilityChanged,
}

impl operation_queue::Operation for Operation {
    fn lamport_timestamp(&self) -> clock::Lamport {
        match self {
            Operation::Buffer(_) => {
                unreachable!("buffer operations should never be deferred at this layer")
            }
            Operation::UpdateDiagnostics {
                lamport_timestamp, ..
            }
            | Operation::UpdateSelections {
                lamport_timestamp, ..
            }
            | Operation::UpdateCompletionTriggers {
                lamport_timestamp, ..
            }
            | Operation::UpdateLineEnding {
                lamport_timestamp, ..
            } => *lamport_timestamp,
        }
    }
}

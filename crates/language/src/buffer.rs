mod autoindent;
mod basic_types;
mod buffer_construction;
mod buffer_diff_dirty;
mod buffer_metadata;
mod buffer_operations;
mod buffer_parsing;
mod buffer_preview;
mod buffer_selections_edits;
mod buffer_test_helpers;
mod buffer_transactions;
mod buffer_undo_completion;
mod buffer_util_impls;
mod chunks;
mod edited_snapshot;
mod file;
mod highlighted_text;
mod operations;
pub mod row_chunk;
mod snapshot_autoindent;
mod snapshot_brackets;
mod snapshot_diagnostics;
mod snapshot_navigation;
mod snapshot_outline;
mod snapshot_syntax;
mod snapshot_text_objects;
mod utilities;

use buffer_util_impls::offset_in_sub_ranges;
pub use edited_snapshot::EditedBufferSnapshot;

use crate::{
    ByteContent, DebuggerTextObject, LanguageScope, ModelineSettings, Outline, OutlineConfig,
    PLAIN_TEXT, RunnableTag, TextObject, TreeSitterOptions, analyze_byte_content,
    diagnostic_set::{DiagnosticEntry, DiagnosticEntryRef, DiagnosticGroup},
    language_settings::{AutoIndentMode, LanguageSettings},
    outline::OutlineItem,
    row_chunk::RowChunks,
    runnable::{self, RunnableRange},
    syntax_map::{
        MAX_BYTES_TO_QUERY, SyntaxLayer, SyntaxMap, SyntaxMapCapture, SyntaxMapCaptures,
        SyntaxMapMatch, SyntaxMapMatches, SyntaxSnapshot, ToTreeSitterPoint,
    },
    text_diff::text_diff,
    unified_diff_with_offsets,
};
pub use crate::{
    Grammar, HighlightId, HighlightMap, Language, LanguageRegistry, diagnostic_set::DiagnosticSet,
    proto,
};

use anyhow::{Context as _, Result};
use clock::Lamport;
pub use clock::ReplicaId;
use collections::{HashMap, HashSet};
use encoding_rs::Encoding;
use fs::MTime;
use futures::channel::oneshot;
use futures_lite::future::yield_now;
use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, HighlightStyle, SharedString, Task,
};

use lsp::LanguageServerId;
use parking_lot::Mutex;
use settings::WorktreeId;
use std::{
    any::Any,
    borrow::Cow,
    cell::Cell,
    cmp::{self, Ordering, Reverse},
    collections::{BTreeMap, BTreeSet},
    future::Future,
    iter::{self, Iterator, Peekable},
    mem,
    num::NonZeroU32,
    ops::{Deref, Range},
    path::PathBuf,
    rc,
    sync::Arc,
    time::{Duration, Instant},
    vec,
};
use sum_tree::TreeMap;
use text::operation_queue::OperationQueue;
use text::*;
pub use text::{
    Anchor, Bias, Buffer as TextBuffer, BufferId, BufferSnapshot as TextBufferSnapshot, Edit,
    LineIndent, OffsetRangeExt, OffsetUtf16, Patch, Point, PointUtf16, Rope, Selection,
    SelectionGoal, Subscription, TextDimension, TextSummary, ToOffset, ToOffsetUtf16, ToPoint,
    ToPointUtf16, Transaction, TransactionId, Unclipped,
};
use theme::{ActiveTheme as _, SyntaxTheme};
#[cfg(any(test, feature = "test-support"))]
use util::RandomCharIter;
use util::{RangeExt, debug_panic, maybe, paths::PathStyle, rel_path::RelPath};

#[cfg(any(test, feature = "test-support"))]
pub use {tree_sitter_python, tree_sitter_rust, tree_sitter_typescript};

pub use basic_types::{
    BracketMatch, BufferRow, Capability, CharKind, CharScopeContext, CursorShape, IndentKind,
    IndentSize, ParseStatus,
};
pub use chunks::{BufferChunks, Chunk, Diff};
pub use file::{BufferEditSource, DiskState, File, LocalFile};
pub use highlighted_text::{EditPreview, HighlightedText, HighlightedTextBuilder, Runnable};
pub use lsp::DiagnosticSeverity;
pub use operations::{BufferEvent, Operation};
#[cfg(any(test, feature = "test-support"))]
pub use utilities::TestFile;
pub(crate) use utilities::contiguous_ranges;
pub use utilities::{CharClassifier, trailing_whitespace_ranges};

/// An in-memory representation of a source code file, including its text,
/// syntax trees, git status, and diagnostics.
pub struct Buffer {
    text: TextBuffer,
    branch_state: Option<BufferBranchState>,
    /// Filesystem state, `None` when there is no path.
    file: Option<Arc<dyn File>>,
    /// The mtime of the file when this buffer was last loaded from
    /// or saved to disk.
    saved_mtime: Option<MTime>,
    /// The version vector when this buffer was last loaded from
    /// or saved to disk.
    saved_version: clock::Global,
    preview_version: clock::Global,
    transaction_depth: usize,
    was_dirty_before_starting_transaction: Option<bool>,
    reload_task: Option<Task<Result<()>>>,
    language: Option<Arc<Language>>,
    autoindent_requests: Vec<Arc<AutoindentRequest>>,
    wait_for_autoindent_txs: Vec<oneshot::Sender<()>>,
    pending_autoindent: Option<Task<()>>,
    sync_parse_timeout: Option<Duration>,
    syntax_map: Mutex<SyntaxMap>,
    reparse: Option<Task<()>>,
    parse_status: (watch::Sender<ParseStatus>, watch::Receiver<ParseStatus>),
    non_text_state_update_count: usize,
    diagnostics: TreeMap<LanguageServerId, DiagnosticSet>,
    remote_selections: TreeMap<ReplicaId, SelectionSet>,
    diagnostics_timestamp: clock::Lamport,
    completion_triggers: BTreeSet<String>,
    completion_triggers_per_language_server: HashMap<LanguageServerId, BTreeSet<String>>,
    completion_triggers_timestamp: clock::Lamport,
    deferred_ops: OperationQueue<Operation>,
    capability: Capability,
    has_conflict: bool,
    /// Memoize calls to has_changes_since(saved_version).
    /// The contents of a cell are (self.version, has_changes) at the time of a last call.
    has_unsaved_edits: Cell<(clock::Global, bool)>,
    change_bits: Vec<rc::Weak<Cell<bool>>>,
    modeline: Option<Arc<ModelineSettings>>,
    _subscriptions: Vec<gpui::Subscription>,
    tree_sitter_data: Arc<TreeSitterData>,
    encoding: &'static Encoding,
    has_bom: bool,
    reload_with_encoding_txns: HashMap<TransactionId, (&'static Encoding, bool)>,
}

#[derive(Debug)]
pub struct TreeSitterData {
    chunks: RowChunks,
    brackets_by_chunks: Mutex<Vec<Option<Vec<BracketMatch<usize>>>>>,
}

const MAX_ROWS_IN_A_CHUNK: u32 = 50;

impl TreeSitterData {
    fn clear(&mut self, snapshot: &text::BufferSnapshot) {
        self.chunks = RowChunks::new(&snapshot, MAX_ROWS_IN_A_CHUNK);
        self.brackets_by_chunks.get_mut().clear();
        self.brackets_by_chunks
            .get_mut()
            .resize(self.chunks.len(), None);
    }

    fn new(snapshot: &text::BufferSnapshot) -> Self {
        let chunks = RowChunks::new(&snapshot, MAX_ROWS_IN_A_CHUNK);
        Self {
            brackets_by_chunks: Mutex::new(vec![None; chunks.len()]),
            chunks,
        }
    }

    fn version(&self) -> &clock::Global {
        self.chunks.version()
    }
}

struct BufferBranchState {
    base_buffer: Entity<Buffer>,
    merged_operations: Vec<Lamport>,
}

/// An immutable, cheaply cloneable representation of a fixed
/// state of a buffer.
pub struct BufferSnapshot {
    pub text: text::BufferSnapshot,
    pub(crate) syntax: SyntaxSnapshot,
    tree_sitter_data: Arc<TreeSitterData>,
    diagnostics: TreeMap<LanguageServerId, DiagnosticSet>,
    remote_selections: TreeMap<ReplicaId, SelectionSet>,
    language: Option<Arc<Language>>,
    file: Option<Arc<dyn File>>,
    non_text_state_update_count: usize,
    pub capability: Capability,
    modeline: Option<Arc<ModelineSettings>>,
}

#[derive(Clone, Debug)]
struct SelectionSet {
    line_mode: bool,
    cursor_shape: CursorShape,
    selections: Arc<[Selection<Anchor>]>,
    lamport_timestamp: clock::Lamport,
}

/// The auto-indent behavior associated with an editing operation.
/// For some editing operations, each affected line of text has its
/// indentation recomputed. For other operations, the entire block
/// of edited text is adjusted uniformly.
#[derive(Clone, Debug)]
pub enum AutoindentMode {
    /// Indent each line of inserted text.
    EachLine,
    /// Apply the same indentation adjustment to all of the lines
    /// in a given insertion.
    Block {
        /// The original indentation column of the first line of each
        /// insertion, if it has been copied.
        ///
        /// Knowing this makes it possible to preserve the relative indentation
        /// of every line in the insertion from when it was copied.
        ///
        /// If the original indent column is `a`, and the first line of insertion
        /// is then auto-indented to column `b`, then every other line of
        /// the insertion will be auto-indented to column `b - a`
        original_indent_columns: Vec<Option<u32>>,
    },
}

#[derive(Clone)]
struct AutoindentRequest {
    before_edit: BufferSnapshot,
    entries: Vec<AutoindentRequestEntry>,
    is_block_mode: bool,
    ignore_empty_lines: bool,
}

#[derive(Debug, Clone)]
struct AutoindentRequestEntry {
    /// A range of the buffer whose indentation should be adjusted.
    range: Range<Anchor>,
    /// The row of the edit start in the buffer before the edit was applied.
    /// This is stored here because the anchor in range is created after
    /// the edit, so it cannot be used with the before_edit snapshot.
    old_row: Option<u32>,
    indent_size: IndentSize,
    original_indent_column: Option<u32>,
}

#[derive(Debug)]
struct IndentSuggestion {
    basis_row: u32,
    delta: Ordering,
    within_error: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DiagnosticEndpoint {
    offset: usize,
    is_start: bool,
    underline: bool,
    severity: DiagnosticSeverity,
    is_unnecessary: bool,
}

/// A configuration to use when producing styled text chunks.
#[derive(Clone, Copy)]
pub struct LanguageAwareStyling {
    /// Whether to highlight text chunks using tree-sitter.
    pub tree_sitter: bool,
    /// Whether to highlight text chunks based on the diagnostics data.
    pub diagnostics: bool,
}

pub struct WordsQuery<'a> {
    /// Only returns words with all chars from the fuzzy string in them.
    pub fuzzy_contents: Option<&'a str>,
    /// Skips words that start with a digit.
    pub skip_digits: bool,
    /// Buffer offset range, to look for words.
    pub range: Range<usize>,
}

fn indent_size_for_line(text: &text::BufferSnapshot, row: u32) -> IndentSize {
    indent_size_for_text(text.chars_at(Point::new(row, 0)))
}

fn indent_size_for_text(text: impl Iterator<Item = char>) -> IndentSize {
    let mut result = IndentSize::spaces(0);
    for c in text {
        let kind = match c {
            ' ' => IndentKind::Space,
            '\t' => IndentKind::Tab,
            _ => break,
        };
        if result.len == 0 {
            result.kind = kind;
        }
        result.len += 1;
    }
    result
}

impl Clone for BufferSnapshot {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            syntax: self.syntax.clone(),
            file: self.file.clone(),
            remote_selections: self.remote_selections.clone(),
            diagnostics: self.diagnostics.clone(),
            language: self.language.clone(),
            tree_sitter_data: self.tree_sitter_data.clone(),
            non_text_state_update_count: self.non_text_state_update_count,
            capability: self.capability,
            modeline: self.modeline.clone(),
        }
    }
}

impl Deref for BufferSnapshot {
    type Target = text::BufferSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

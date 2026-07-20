use super::*;
use crate::{
    JoinLines,
    code_context_menus::CodeContextMenu,
    edit_prediction_tests::FakeEditPredictionDelegate,
    element::{StickyHeader, header_jump_data},
    linked_editing_ranges::LinkedEditingRanges,
    runnables::RunnableTasks,
    scroll::scroll_amount::ScrollAmount,
    test::{
        assert_text_with_selections, build_editor, editor_content_with_blocks,
        editor_lsp_test_context::{EditorLspTestContext, git_commit_lang},
        editor_test_context::EditorTestContext,
        select_ranges,
    },
};
use buffer_diff::{BufferDiff, DiffHunkSecondaryStatus, DiffHunkStatus, DiffHunkStatusKind};
use collections::HashMap;
use futures::{StreamExt, channel::oneshot};
use gpui::{
    BackgroundExecutor, DismissEvent, Task, TaskExt, TestAppContext, UpdateGlobal,
    VisualTestContext, WindowBounds, WindowOptions, div,
};
use indoc::indoc;
use language::{
    BracketPair, BracketPairConfig,
    Capability::ReadWrite,
    ContextLocation, ContextProvider, DiagnosticSourceKind, FakeLspAdapter, IndentGuideSettings,
    LanguageConfig, LanguageConfigOverride, LanguageMatcher, LanguageName, LanguageQueries,
    LanguageToolchainStore, Override, Point,
    language_settings::{
        CompletionSettingsContent, FormatterList, LanguageSettingsContent, LspInsertMode,
    },
    tree_sitter_python,
};
use language_settings::Formatter;
use languages::markdown_lang;
use languages::rust_lang;
use lsp::{CompletionParams, DEFAULT_LSP_REQUEST_TIMEOUT};
use multi_buffer::{IndentGuide, MultiBuffer, MultiBufferOffset, MultiBufferOffsetUtf16, PathKey};
use parking_lot::Mutex;
use pretty_assertions::{assert_eq, assert_ne};
use project::{
    FakeFs, Project, ProjectPath,
    bookmark_store::SerializedBookmark,
    debugger::breakpoint_store::{BreakpointState, SourceBreakpoint},
    project_settings::LspSettings,
    trusted_worktrees::{PathTrust, TrustedWorktrees},
};
use serde_json::{self, json};
use settings::{
    AllLanguageSettingsContent, DelayMs, EditorSettingsContent, GlobalLspSettingsContent,
    GoToDefinitionScrollStrategy, IndentGuideBackgroundColoring, IndentGuideColoring,
    InlayHintSettingsContent, ProjectSettingsContent, ScrollBeyondLastLine, SearchSettingsContent,
    SettingsContent, SettingsStore,
};
use std::{borrow::Cow, sync::Arc};
use std::{cell::RefCell, future::Future, rc::Rc, sync::atomic::AtomicBool, time::Instant};
use std::{
    iter,
    sync::atomic::{self, AtomicUsize},
};
use task::TaskVariables;
use test::build_editor_with_project;
use unindent::Unindent;
use util::{
    assert_set_eq, path,
    rel_path::rel_path,
    test::{TextRangeMarker, marked_text_ranges, marked_text_ranges_by, sample_text},
};
use workspace::{
    CloseActiveItem, CloseAllItems, CloseOtherItems, MultiWorkspace, NavigationEntry, OpenOptions,
    ToolbarItemLocation, ViewId,
    item::{FollowEvent, FollowableItem, Item, ItemHandle, SaveOptions},
    register_project_item,
};

#[cfg(any(test, feature = "test-support"))]
pub mod property_test;

#[path = "editor_tests/support.rs"]
mod support;
use support::*;

#[path = "editor_tests/add_selection_above_below.rs"]
mod add_selection_above_below;
#[path = "editor_tests/add_selection_and_matches.rs"]
mod add_selection_and_matches;
#[path = "editor_tests/autoclose_delete_surround.rs"]
mod autoclose_delete_surround;
#[path = "editor_tests/autoclose_remaining.rs"]
mod autoclose_remaining;
#[path = "editor_tests/autoindent_autoclose_intro.rs"]
mod autoindent_autoclose_intro;
#[path = "editor_tests/autoindent_remaining.rs"]
mod autoindent_remaining;
#[path = "editor_tests/backspace_join_lines.rs"]
mod backspace_join_lines;
#[path = "editor_tests/bash_indent.rs"]
mod bash_indent;
#[path = "editor_tests/bash_newline.rs"]
mod bash_newline;
#[path = "editor_tests/basic_navigation.rs"]
mod basic_navigation;
#[path = "editor_tests/block_operations.rs"]
mod block_operations;
#[path = "editor_tests/bookmark_context.rs"]
mod bookmark_context;
#[path = "editor_tests/bookmark_core.rs"]
mod bookmark_core;
#[path = "editor_tests/bookmark_navigation.rs"]
mod bookmark_navigation;
#[path = "editor_tests/breakpoints_basic.rs"]
mod breakpoints_basic;
#[path = "editor_tests/breakpoints_state.rs"]
mod breakpoints_state;
#[path = "editor_tests/clipboard_save.rs"]
mod clipboard_save;
#[path = "editor_tests/clipboard_selection.rs"]
mod clipboard_selection;
#[path = "editor_tests/comment_blocks_excerpts.rs"]
mod comment_blocks_excerpts;
#[path = "editor_tests/comment_lines.rs"]
mod comment_lines;
#[path = "editor_tests/completion_additional_edits.rs"]
mod completion_additional_edits;
#[path = "editor_tests/completion_commands.rs"]
mod completion_commands;
#[path = "editor_tests/completion_core.rs"]
mod completion_core;
#[path = "editor_tests/completion_edges.rs"]
mod completion_edges;
#[path = "editor_tests/completion_extra_word_chars.rs"]
mod completion_extra_word_chars;
#[path = "editor_tests/completion_modes.rs"]
mod completion_modes;
#[path = "editor_tests/completion_replace.rs"]
mod completion_replace;
#[path = "editor_tests/completion_resolve.rs"]
mod completion_resolve;
#[path = "editor_tests/cursor_line_word.rs"]
mod cursor_line_word;
#[path = "editor_tests/cursor_movement_basic.rs"]
mod cursor_movement_basic;
#[path = "editor_tests/delete_boundaries.rs"]
mod delete_boundaries;
#[path = "editor_tests/delete_brackets_words.rs"]
mod delete_brackets_words;
#[path = "editor_tests/diagnostics_hunks.rs"]
mod diagnostics_hunks;
#[path = "editor_tests/diff_display_remaining.rs"]
mod diff_display_remaining;
#[path = "editor_tests/diff_edit_edges.rs"]
mod diff_edit_edges;
#[path = "editor_tests/diff_edit_stage.rs"]
mod diff_edit_stage;
#[path = "editor_tests/diff_expansion.rs"]
mod diff_expansion;
#[path = "editor_tests/diff_multibuffer.rs"]
mod diff_multibuffer;
#[path = "editor_tests/diff_reverts.rs"]
mod diff_reverts;
#[path = "editor_tests/diff_review_button.rs"]
mod diff_review_button;
#[path = "editor_tests/diff_review_comments.rs"]
mod diff_review_comments;
#[path = "editor_tests/diff_review_drag.rs"]
mod diff_review_drag;
#[path = "editor_tests/diff_review_orphaned.rs"]
mod diff_review_orphaned;
#[path = "editor_tests/diff_review_overlay.rs"]
mod diff_review_overlay;
#[path = "editor_tests/diff_toggle_adjacent.rs"]
mod diff_toggle_adjacent;
#[path = "editor_tests/edit_prediction_highlights.rs"]
mod edit_prediction_highlights;
#[path = "editor_tests/editor_restore_data.rs"]
mod editor_restore_data;
#[path = "editor_tests/editor_ui_interactions.rs"]
mod editor_ui_interactions;
#[path = "editor_tests/events_input.rs"]
mod events_input;
#[path = "editor_tests/file_syntax_basics.rs"]
mod file_syntax_basics;
#[path = "editor_tests/folded_buffer_navigation.rs"]
mod folded_buffer_navigation;
#[path = "editor_tests/folded_buffers.rs"]
mod folded_buffers;
#[path = "editor_tests/folding_basic.rs"]
mod folding_basic;
#[path = "editor_tests/folding_multiline.rs"]
mod folding_multiline;
#[path = "editor_tests/following.rs"]
mod following;
#[path = "editor_tests/format_on_save.rs"]
mod format_on_save;
#[path = "editor_tests/format_requests.rs"]
mod format_requests;
#[path = "editor_tests/formatter_selection.rs"]
mod formatter_selection;
#[path = "editor_tests/goto_definition.rs"]
mod goto_definition;
#[path = "editor_tests/highlight_json.rs"]
mod highlight_json;
#[path = "editor_tests/indent_guides_active.rs"]
mod indent_guides_active;
#[path = "editor_tests/indent_guides_basic.rs"]
mod indent_guides_basic;
#[path = "editor_tests/indent_outdent.rs"]
mod indent_outdent;
#[path = "editor_tests/inlay_hint_click.rs"]
mod inlay_hint_click;
#[path = "editor_tests/join_lines_comments.rs"]
mod join_lines_comments;
#[path = "editor_tests/larger_syntax_node_movement.rs"]
mod larger_syntax_node_movement;
#[path = "editor_tests/larger_syntax_node_selection.rs"]
mod larger_syntax_node_selection;
#[path = "editor_tests/line_operations.rs"]
mod line_operations;
#[path = "editor_tests/linked_edits.rs"]
mod linked_edits;
#[path = "editor_tests/local_worktree_trust.rs"]
mod local_worktree_trust;
#[path = "editor_tests/lsp_formatting_restart.rs"]
mod lsp_formatting_restart;
#[path = "editor_tests/manual_formatting.rs"]
mod manual_formatting;
#[path = "editor_tests/markdown_list_indent.rs"]
mod markdown_list_indent;
#[path = "editor_tests/markdown_newline_lists.rs"]
mod markdown_newline_lists;
#[path = "editor_tests/markdown_paste.rs"]
mod markdown_paste;
#[path = "editor_tests/mixed_completion_snippet.rs"]
mod mixed_completion_snippet;
#[path = "editor_tests/modal_popovers.rs"]
mod modal_popovers;
#[path = "editor_tests/multi_formatter.rs"]
mod multi_formatter;
#[path = "editor_tests/multibuffer_reference_navigation.rs"]
mod multibuffer_reference_navigation;
#[path = "editor_tests/multibuffer_sticky_navigation.rs"]
mod multibuffer_sticky_navigation;
#[path = "editor_tests/multiline_completion.rs"]
mod multiline_completion;
#[path = "editor_tests/newline_comments.rs"]
mod newline_comments;
#[path = "editor_tests/newline_core.rs"]
mod newline_core;
#[path = "editor_tests/newline_multibuffer.rs"]
mod newline_multibuffer;
#[path = "editor_tests/prettier_formatting.rs"]
mod prettier_formatting;
#[path = "editor_tests/pulling_diagnostics.rs"]
mod pulling_diagnostics;
#[path = "editor_tests/python_indent.rs"]
mod python_indent;
#[path = "editor_tests/python_newline_markdown.rs"]
mod python_newline_markdown;
#[path = "editor_tests/range_formatting.rs"]
mod range_formatting;
#[path = "editor_tests/reference_task_context.rs"]
mod reference_task_context;
#[path = "editor_tests/rename_codelens.rs"]
mod rename_codelens;
#[path = "editor_tests/restore_align.rs"]
mod restore_align;
#[path = "editor_tests/rewrap.rs"]
mod rewrap;
#[path = "editor_tests/rewrap_blocks.rs"]
mod rewrap_blocks;
#[path = "editor_tests/rewrap_comments.rs"]
mod rewrap_comments;
#[path = "editor_tests/scroll_movement.rs"]
mod scroll_movement;
#[path = "editor_tests/select_delimiters.rs"]
mod select_delimiters;
#[path = "editor_tests/select_previous_and_undo.rs"]
mod select_previous_and_undo;
#[path = "editor_tests/selection_columnar_markdown.rs"]
mod selection_columnar_markdown;
#[path = "editor_tests/selection_cursors.rs"]
mod selection_cursors;
#[path = "editor_tests/selection_diagnostics.rs"]
mod selection_diagnostics;
#[path = "editor_tests/selection_split.rs"]
mod selection_split;
#[path = "editor_tests/signature_help_display.rs"]
mod signature_help_display;
#[path = "editor_tests/signature_help_input.rs"]
mod signature_help_input;
#[path = "editor_tests/snippet_inlay_timeout.rs"]
mod snippet_inlay_timeout;
#[path = "editor_tests/snippet_insertion.rs"]
mod snippet_insertion;
#[path = "editor_tests/snippet_menu_refresh.rs"]
mod snippet_menu_refresh;
#[path = "editor_tests/snippet_remaining.rs"]
mod snippet_remaining;
#[path = "editor_tests/sticky_headers_remaining.rs"]
mod sticky_headers_remaining;
#[path = "editor_tests/sticky_scroll.rs"]
mod sticky_scroll;
#[path = "editor_tests/syntax_highlighting_edges.rs"]
mod syntax_highlighting_edges;
#[path = "editor_tests/syntax_movement.rs"]
mod syntax_movement;
#[path = "editor_tests/syntax_node_selection.rs"]
mod syntax_node_selection;
#[path = "editor_tests/syntax_selection_remaining.rs"]
mod syntax_selection_remaining;
#[path = "editor_tests/tab_indent.rs"]
mod tab_indent;
#[path = "editor_tests/text_case_manipulate.rs"]
mod text_case_manipulate;
#[path = "editor_tests/text_conversion.rs"]
mod text_conversion;
#[path = "editor_tests/text_manipulation.rs"]
mod text_manipulation;
#[path = "editor_tests/text_wrap_tags.rs"]
mod text_wrap_tags;
#[path = "editor_tests/transpose.rs"]
mod transpose;
#[path = "editor_tests/word_completion_menu.rs"]
mod word_completion_menu;
#[path = "editor_tests/word_completion_settings.rs"]
mod word_completion_settings;

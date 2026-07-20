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

fn display_ranges(editor: &Editor, cx: &mut Context<'_, Editor>) -> Vec<Range<DisplayPoint>> {
    editor
        .selections
        .display_ranges(&editor.display_snapshot(cx))
}

#[cfg(any(test, feature = "test-support"))]
pub mod property_test;

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
#[path = "editor_tests/rewrap.rs"]
mod rewrap;
#[path = "editor_tests/rewrap_blocks.rs"]
mod rewrap_blocks;
#[path = "editor_tests/rewrap_comments.rs"]
mod rewrap_comments;
#[path = "editor_tests/scroll_movement.rs"]
mod scroll_movement;
#[path = "editor_tests/select_previous_and_undo.rs"]
mod select_previous_and_undo;
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

fn completion_menu_entries(menu: &CompletionsMenu) -> Vec<String> {
    let entries = menu.entries.borrow();
    entries
        .iter()
        .filter_map(|entry| entry.as_match().map(|m| m.string.clone()))
        .collect()
}

async fn setup_indent_guides_editor(
    text: &str,
    cx: &mut TestAppContext,
) -> (BufferId, EditorTestContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let buffer_id = cx.update_editor(|editor, window, cx| {
        editor.set_text(text, window, cx);
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .read(cx)
            .remote_id()
    });

    (buffer_id, cx)
}

fn assert_indent_guides(
    range: Range<u32>,
    expected: Vec<IndentGuide>,
    active_indices: Option<Vec<usize>>,
    cx: &mut EditorTestContext,
) {
    let indent_guides = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx).display_snapshot;
        let mut indent_guides: Vec<_> = crate::indent_guides::indent_guides_in_range(
            editor,
            MultiBufferRow(range.start)..MultiBufferRow(range.end),
            true,
            &snapshot,
            cx,
        );

        indent_guides.sort_by(|a, b| {
            a.depth.cmp(&b.depth).then(
                a.start_row
                    .cmp(&b.start_row)
                    .then(a.end_row.cmp(&b.end_row)),
            )
        });
        indent_guides
    });

    if let Some(expected) = active_indices {
        let active_indices = cx.update_editor(|editor, window, cx| {
            let snapshot = editor.snapshot(window, cx).display_snapshot;
            editor.find_active_indent_guide_indices(&indent_guides, &snapshot, window, cx)
        });

        assert_eq!(
            active_indices.unwrap().into_iter().collect::<Vec<_>>(),
            expected,
            "Active indent guide indices do not match"
        );
    }

    assert_eq!(indent_guides, expected, "Indent guides do not match");
}

fn indent_guide(buffer_id: BufferId, start_row: u32, end_row: u32, depth: u32) -> IndentGuide {
    IndentGuide {
        buffer_id,
        start_row: MultiBufferRow(start_row),
        end_row: MultiBufferRow(end_row),
        depth,
        tab_size: 4,
        settings: IndentGuideSettings {
            enabled: true,
            line_width: 1,
            active_line_width: 1,
            coloring: IndentGuideColoring::default(),
            background_coloring: IndentGuideBackgroundColoring::default(),
        },
    }
}

#[track_caller]
fn assert_breakpoint(
    breakpoints: &BTreeMap<Arc<Path>, Vec<SourceBreakpoint>>,
    path: &Arc<Path>,
    expected: Vec<(u32, Breakpoint)>,
) {
    if expected.is_empty() {
        assert!(!breakpoints.contains_key(path), "{}", path.display());
    } else {
        let mut breakpoint = breakpoints
            .get(path)
            .unwrap()
            .iter()
            .map(|breakpoint| {
                (
                    breakpoint.row,
                    Breakpoint {
                        message: breakpoint.message.clone(),
                        state: breakpoint.state,
                        condition: breakpoint.condition.clone(),
                        hit_condition: breakpoint.hit_condition.clone(),
                    },
                )
            })
            .collect::<Vec<_>>();

        breakpoint.sort_by_key(|(cached_position, _)| *cached_position);

        assert_eq!(expected, breakpoint);
    }
}

fn add_log_breakpoint_at_cursor(
    editor: &mut Editor,
    log_message: &str,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let (anchor, bp) = editor
        .breakpoints_at_cursors(window, cx)
        .first()
        .and_then(|(anchor, bp)| bp.as_ref().map(|bp| (*anchor, bp.clone())))
        .unwrap_or_else(|| {
            let snapshot = editor.snapshot(window, cx);
            let cursor_position: Point =
                editor.selections.newest(&snapshot.display_snapshot).head();

            let breakpoint_position = snapshot
                .buffer_snapshot()
                .anchor_before(Point::new(cursor_position.row, 0));

            (breakpoint_position, Breakpoint::new_log(log_message))
        });

    editor.edit_breakpoint_at_anchor(
        anchor,
        bp,
        BreakpointEditAction::EditLogMessage(log_message.into()),
        cx,
    );
}

fn empty_range(row: usize, column: usize) -> Range<DisplayPoint> {
    let point = DisplayPoint::new(DisplayRow(row as u32), column as u32);
    point..point
}

#[track_caller]
fn assert_selection_ranges(marked_text: &str, editor: &mut Editor, cx: &mut Context<Editor>) {
    let (text, ranges) = marked_text_ranges(marked_text, true);
    assert_eq!(editor.text(cx), text);
    assert_eq!(
        editor.selections.ranges(&editor.display_snapshot(cx)),
        ranges
            .iter()
            .map(|range| MultiBufferOffset(range.start)..MultiBufferOffset(range.end))
            .collect::<Vec<_>>(),
        "Assert selections are {}",
        marked_text
    );
}

pub fn handle_signature_help_request(
    cx: &mut EditorLspTestContext,
    mocked_response: lsp::SignatureHelp,
) -> impl Future<Output = ()> + use<> {
    let mut request =
        cx.set_request_handler::<lsp::request::SignatureHelpRequest, _, _>(move |_, _, _| {
            let mocked_response = mocked_response.clone();
            async move { Ok(Some(mocked_response)) }
        });

    async move {
        request.next().await;
    }
}

#[track_caller]
pub fn check_displayed_completions(expected: Vec<&'static str>, cx: &mut EditorLspTestContext) {
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow().as_ref() {
            let entries = menu.entries.borrow();
            let entries = entries
                .iter()
                .filter_map(|entry| entry.as_match())
                .map(|m| m.string.as_str())
                .collect::<Vec<_>>();
            assert_eq!(entries, expected);
        } else {
            panic!("Expected completions menu");
        }
    });
}

fn handle_resolve_completion_request(
    cx: &mut EditorLspTestContext,
    edits: Option<Vec<(&'static str, &'static str)>>,
) -> impl Future<Output = ()> {
    let edits = edits.map(|edits| {
        edits
            .iter()
            .map(|(marked_string, new_text)| {
                let (_, marked_ranges) = marked_text_ranges(marked_string, false);
                let replace_range = cx.to_lsp_range(
                    MultiBufferOffset(marked_ranges[0].start)
                        ..MultiBufferOffset(marked_ranges[0].end),
                );
                lsp::TextEdit::new(replace_range, new_text.to_string())
            })
            .collect::<Vec<_>>()
    });

    let mut request =
        cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(move |_, _, _| {
            let edits = edits.clone();
            async move {
                Ok(lsp::CompletionItem {
                    additional_text_edits: edits,
                    ..Default::default()
                })
            }
        });

    async move {
        request.next().await;
    }
}

pub(crate) fn update_test_language_settings(
    cx: &mut TestAppContext,
    f: &dyn Fn(&mut AllLanguageSettingsContent),
) {
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, &|settings: &mut SettingsContent| {
                f(&mut settings.project.all_languages)
            });
        });
    });
}

pub(crate) fn update_test_project_settings(
    cx: &mut TestAppContext,
    f: &dyn Fn(&mut ProjectSettingsContent),
) {
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| f(&mut settings.project));
        });
    });
}

pub(crate) fn update_test_editor_settings(
    cx: &mut TestAppContext,
    f: &dyn Fn(&mut EditorSettingsContent),
) {
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| f(&mut settings.editor));
        })
    })
}

pub(crate) fn init_test(cx: &mut TestAppContext, f: fn(&mut AllLanguageSettingsContent)) {
    cx.update(|cx| {
        assets::Assets.load_test_fonts(cx);
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        crate::init(cx);
    });
    zlog::init_test();
    update_test_language_settings(cx, &f);
}

#[track_caller]
fn assert_hunk_revert(
    not_reverted_text_with_selections: &str,
    expected_hunk_statuses_before: Vec<DiffHunkStatusKind>,
    expected_reverted_text_with_selections: &str,
    base_text: &str,
    cx: &mut EditorLspTestContext,
) {
    cx.set_state(not_reverted_text_with_selections);
    cx.set_head_text(base_text);
    cx.executor().run_until_parked();

    let actual_hunk_statuses_before = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let reverted_hunk_statuses = snapshot
            .buffer_snapshot()
            .diff_hunks_in_range(MultiBufferOffset(0)..snapshot.buffer_snapshot().len())
            .map(|hunk| hunk.status().kind)
            .collect::<Vec<_>>();

        editor.git_restore(&Default::default(), window, cx);
        reverted_hunk_statuses
    });
    cx.executor().run_until_parked();
    cx.assert_editor_state(expected_reverted_text_with_selections);
    assert_eq!(actual_hunk_statuses_before, expected_hunk_statuses_before);
}

/// Helper function to create a DiffHunkKey for testing.
/// Uses Anchor::Min as a placeholder anchor since these tests don't need
/// real buffer positioning.
fn test_hunk_key(file_path: &str) -> DiffHunkKey {
    DiffHunkKey {
        file_path: if file_path.is_empty() {
            Arc::from(util::rel_path::RelPath::empty())
        } else {
            Arc::from(util::rel_path::RelPath::unix(file_path).unwrap())
        },
        hunk_start_anchor: Anchor::Min,
    }
}

/// Helper function to create a DiffHunkKey with a specific anchor for testing.
fn test_hunk_key_with_anchor(file_path: &str, anchor: Anchor) -> DiffHunkKey {
    DiffHunkKey {
        file_path: if file_path.is_empty() {
            Arc::from(util::rel_path::RelPath::empty())
        } else {
            Arc::from(util::rel_path::RelPath::unix(file_path).unwrap())
        },
        hunk_start_anchor: anchor,
    }
}

/// Helper function to add a review comment with default anchors for testing.
fn add_test_comment(
    editor: &mut Editor,
    key: DiffHunkKey,
    comment: &str,
    cx: &mut Context<Editor>,
) -> usize {
    editor.add_review_comment(key, comment.to_string(), Anchor::Min..Anchor::Max, cx)
}

#[gpui::test]
async fn test_move_to_start_end_of_larger_syntax_node_single_cursor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        fn main() {
            let x = foo(1, 2);
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: Move to end of syntax nodes
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 16)..DisplayPoint::new(DisplayRow(1), 16)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(ˇ1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1ˇ, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2)ˇ;
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);ˇ
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                }ˇ
            "#},
            cx,
        );
    });

    // Test case 2: Move to start of syntax nodes
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 20)..DisplayPoint::new(DisplayRow(1), 20)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2ˇ);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = fooˇ(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = ˇfoo(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    ˇlet x = foo(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() ˇ{
                    let x = foo(1, 2);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                ˇfn main() {
                    let x = foo(1, 2);
                }
            "#},
            cx,
        );
    });
}

#[gpui::test]
async fn test_move_to_start_end_of_larger_syntax_node_two_cursors(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        fn main() {
            let x = foo(1, 2);
            let y = bar(3, 4);
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: Move to end of syntax nodes with two cursors
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 20)..DisplayPoint::new(DisplayRow(1), 20),
                DisplayPoint::new(DisplayRow(2), 20)..DisplayPoint::new(DisplayRow(2), 20),
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2ˇ);
                    let y = bar(3, 4ˇ);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2)ˇ;
                    let y = bar(3, 4)ˇ;
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);ˇ
                    let y = bar(3, 4);ˇ
                }
            "#},
            cx,
        );
    });

    // Test case 2: Move to start of syntax nodes with two cursors
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 19)..DisplayPoint::new(DisplayRow(1), 19),
                DisplayPoint::new(DisplayRow(2), 19)..DisplayPoint::new(DisplayRow(2), 19),
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, ˇ2);
                    let y = bar(3, ˇ4);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = fooˇ(1, 2);
                    let y = barˇ(3, 4);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = ˇfoo(1, 2);
                    let y = ˇbar(3, 4);
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    ˇlet x = foo(1, 2);
                    ˇlet y = bar(3, 4);
                }
            "#},
            cx,
        );
    });
}

#[gpui::test]
async fn test_move_to_start_end_of_larger_syntax_node_with_selections_and_strings(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        fn main() {
            let x = foo(1, 2);
            let msg = "hello world";
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    // Test case 1: With existing selection, move_to_end keeps selection
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 12)..DisplayPoint::new(DisplayRow(1), 21)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = «foo(1, 2)ˇ»;
                    let msg = "hello world";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = «foo(1, 2)ˇ»;
                    let msg = "hello world";
                }
            "#},
            cx,
        );
    });

    // Test case 2: Move to end within a string
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 15)..DisplayPoint::new(DisplayRow(2), 15)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "ˇhello world";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end_of_larger_syntax_node(&MoveToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "hello worldˇ";
                }
            "#},
            cx,
        );
    });

    // Test case 3: Move to start within a string
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 21)..DisplayPoint::new(DisplayRow(2), 21)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "hello ˇworld";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_start_of_larger_syntax_node(&MoveToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x = foo(1, 2);
                    let msg = "ˇhello world";
                }
            "#},
            cx,
        );
    });
}

#[gpui::test]
async fn test_select_to_start_end_of_larger_syntax_node(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    // Test Group 1.1: Cursor in String - First Jump (Select to End)
    let text = r#"let msg = "foo bar baz";"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 14)..DisplayPoint::new(DisplayRow(0), 14)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "fooˇ bar baz";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo« bar bazˇ»";"#}, cx);
    });

    // Test Group 1.2: Cursor in String - Second Jump (Select to End)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo« bar baz"ˇ»;"#}, cx);
    });

    // Test Group 1.3: Cursor in String - Third Jump (Select to End)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo« bar baz";ˇ»"#}, cx);
    });

    // Test Group 1.4: Cursor in String - First Jump (Select to Start)
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 18)..DisplayPoint::new(DisplayRow(0), 18)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "foo barˇ baz";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = "«ˇfoo bar» baz";"#}, cx);
    });

    // Test Group 1.5: Cursor in String - Second Jump (Select to Start)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let msg = «ˇ"foo bar» baz";"#}, cx);
    });

    // Test Group 1.6: Cursor in String - Third Jump (Select to Start)
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"«ˇlet msg = "foo bar» baz";"#}, cx);
    });

    // Test Group 2.1: Let Statement Progression (Select to End)
    let text = r#"
fn main() {
    let x = "hello";
}
"#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 9)..DisplayPoint::new(DisplayRow(1), 9)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let xˇ = "hello";
                }
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r##"
                fn main() {
                    let x« = "hello";ˇ»
                }
            "##},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                fn main() {
                    let x« = "hello";
                }ˇ»
            "#},
            cx,
        );
    });

    // Test Group 2.2a: From Inside String Content Node To String Content Boundary
    let text = r#"let x = "hello";"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 12)..DisplayPoint::new(DisplayRow(0), 12)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "helˇlo";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "«ˇhel»lo";"#}, cx);
    });

    // Test Group 2.2b: From Edge of String Content Node To String Literal Boundary
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 9)..DisplayPoint::new(DisplayRow(0), 9)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "ˇhello";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_start_of_larger_syntax_node(&SelectToStartOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = «ˇ"»hello";"#}, cx);
    });

    // Test Group 3.1: Create Selection from Cursor (Select to End)
    let text = r#"let x = "hello world";"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 14)..DisplayPoint::new(DisplayRow(0), 14)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "helloˇ world";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "hello« worldˇ»";"#}, cx);
    });

    // Test Group 3.2: Extend Existing Selection (Select to End)
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 11)..DisplayPoint::new(DisplayRow(0), 17)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "he«llo woˇ»rld";"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "he«llo worldˇ»";"#}, cx);
    });

    // Test Group 4.1: Multiple Cursors - All Expand to Different Syntax Nodes
    let text = r#"let x = "hello"; let y = 42;"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                // Cursor inside string content
                DisplayPoint::new(DisplayRow(0), 12)..DisplayPoint::new(DisplayRow(0), 12),
                // Cursor at let statement semicolon
                DisplayPoint::new(DisplayRow(0), 18)..DisplayPoint::new(DisplayRow(0), 18),
                // Cursor inside integer literal
                DisplayPoint::new(DisplayRow(0), 26)..DisplayPoint::new(DisplayRow(0), 26),
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "helˇlo"; lˇet y = 4ˇ2;"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let x = "hel«loˇ»"; l«et y = 42;ˇ»"#}, cx);
    });

    // Test Group 4.2: Multiple Cursors on Separate Lines
    let text = r#"
let x = "hello";
let y = 42;
"#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 12)..DisplayPoint::new(DisplayRow(0), 12),
                DisplayPoint::new(DisplayRow(1), 9)..DisplayPoint::new(DisplayRow(1), 9),
            ]);
        });
    });

    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let x = "helˇlo";
                let y = 4ˇ2;
            "#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
                let x = "hel«loˇ»";
                let y = 4«2ˇ»;
            "#},
            cx,
        );
    });

    // Test Group 5.1: Nested Function Calls
    let text = r#"let result = foo(bar("arg"));"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 22)..DisplayPoint::new(DisplayRow(0), 22)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("ˇarg"));"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("«argˇ»"));"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("«arg"ˇ»));"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let result = foo(bar("«arg")ˇ»);"#}, cx);
    });

    // Test Group 6.1: Block Comments
    let text = r#"let x = /* multi
                             line
                             comment */;"#
        .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 16)..DisplayPoint::new(DisplayRow(0), 16)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
let x = /* multiˇ
line
comment */;"#},
            cx,
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(
            editor,
            indoc! {r#"
let x = /* multi«
line
comment */ˇ»;"#},
            cx,
        );
    });

    // Test Group 6.2: Array/Vector Literals
    let text = r#"let arr = [1, 2, 3];"#.unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language.clone(), cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 11)..DisplayPoint::new(DisplayRow(0), 11)
            ]);
        });
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let arr = [ˇ1, 2, 3];"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let arr = [«1ˇ», 2, 3];"#}, cx);
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_to_end_of_larger_syntax_node(&SelectToEndOfLargerSyntaxNode, window, cx);
    });
    editor.update(cx, |editor, cx| {
        assert_text_with_selections(editor, indoc! {r#"let arr = [«1, 2, 3]ˇ»;"#}, cx);
    });
}

#[gpui::test]
async fn test_restore_and_next(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        one
        two
        three
        four
        five
        "#
    .unindent();

    cx.set_state(
        &r#"
        ONE
        two
        ˇTHREE
        four
        FIVE
        "#
        .unindent(),
    );
    cx.set_head_text(&diff_base);

    cx.update_editor(|editor, window, cx| {
        editor.set_expand_all_diff_hunks(cx);
        editor.restore_and_next(&Default::default(), window, cx);
    });
    cx.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        - one
        + ONE
          two
          three
          four
        - ˇfive
        + FIVE
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.restore_and_next(&Default::default(), window, cx);
    });
    cx.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        - one
        + ONE
          two
          three
          four
          ˇfive
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_align_selections(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // 1) one cursor, no action
    let before = " abc\n  abc\nabc\n     ˇabc";
    cx.set_state(before);
    cx.update_editor(|e, window, cx| e.align_selections(&AlignSelections, window, cx));
    cx.assert_editor_state(before);

    // 2) multiple cursors at different rows
    let before = indoc!(
        r#"
            let aˇbc = 123;
            let  xˇyz = 456;
            let   fˇoo = 789;
            let    bˇar = 0;
        "#
    );
    let after = indoc!(
        r#"
            let a   ˇbc = 123;
            let  x  ˇyz = 456;
            let   f ˇoo = 789;
            let    bˇar = 0;
        "#
    );
    cx.set_state(before);
    cx.update_editor(|e, window, cx| e.align_selections(&AlignSelections, window, cx));
    cx.assert_editor_state(after);

    // 3) multiple selections at different rows
    let before = indoc!(
        r#"
            let «ˇabc» = 123;
            let  «ˇxyz» = 456;
            let   «ˇfoo» = 789;
            let    «ˇbar» = 0;
        "#
    );
    let after = indoc!(
        r#"
            let    «ˇabc» = 123;
            let    «ˇxyz» = 456;
            let    «ˇfoo» = 789;
            let    «ˇbar» = 0;
        "#
    );
    cx.set_state(before);
    cx.update_editor(|e, window, cx| e.align_selections(&AlignSelections, window, cx));
    cx.assert_editor_state(after);

    // 4) multiple selections at different rows, inverted head
    let before = indoc!(
        r#"
            let    «abcˇ» = 123;
            // comment
            let  «xyzˇ» = 456;
            let «fooˇ» = 789;
            let    «barˇ» = 0;
        "#
    );
    let after = indoc!(
        r#"
            let    «abcˇ» = 123;
            // comment
            let    «xyzˇ» = 456;
            let    «fooˇ» = 789;
            let    «barˇ» = 0;
        "#
    );
    cx.set_state(before);
    cx.update_editor(|e, window, cx| e.align_selections(&AlignSelections, window, cx));
    cx.assert_editor_state(after);
}

#[gpui::test]
async fn test_align_selections_multicolumn(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // 1) Multicolumn, one non affected editor row
    let before = indoc!(
        r#"
            name «|ˇ» age «|ˇ» height «|ˇ» note
            Matthew «|ˇ» 7 «|ˇ» 2333 «|ˇ» smart
            Mike «|ˇ» 1234 «|ˇ» 567 «|ˇ» lazy
            Anything that is not selected
            Miles «|ˇ» 88 «|ˇ» 99 «|ˇ» funny
        "#
    );
    let after = indoc!(
        r#"
            name    «|ˇ» age  «|ˇ» height «|ˇ» note
            Matthew «|ˇ» 7    «|ˇ» 2333   «|ˇ» smart
            Mike    «|ˇ» 1234 «|ˇ» 567    «|ˇ» lazy
            Anything that is not selected
            Miles   «|ˇ» 88   «|ˇ» 99     «|ˇ» funny
        "#
    );
    cx.set_state(before);
    cx.update_editor(|e, window, cx| e.align_selections(&AlignSelections, window, cx));
    cx.assert_editor_state(after);

    // 2) not all alignment rows has the number of alignment columns
    let before = indoc!(
        r#"
            name «|ˇ» age «|ˇ» height
            Matthew «|ˇ» 7 «|ˇ» 2333
            Mike «|ˇ» 1234
            Miles «|ˇ» 88 «|ˇ» 99
        "#
    );
    let after = indoc!(
        r#"
            name    «|ˇ» age «|ˇ» height
            Matthew «|ˇ» 7   «|ˇ» 2333
            Mike    «|ˇ» 1234
            Miles   «|ˇ» 88  «|ˇ» 99
        "#
    );
    cx.set_state(before);
    cx.update_editor(|e, window, cx| e.align_selections(&AlignSelections, window, cx));
    cx.assert_editor_state(after);

    // 3) A aligned column shall stay aligned
    let before = indoc!(
        r#"
            $ ˇa    ˇa
            $  ˇa   ˇa
            $   ˇa  ˇa
            $    ˇa ˇa
        "#
    );
    let after = indoc!(
        r#"
            $    ˇa    ˇa
            $    ˇa    ˇa
            $    ˇa    ˇa
            $    ˇa    ˇa
        "#
    );
    cx.set_state(before);
    cx.update_editor(|e, window, cx| e.align_selections(&AlignSelections, window, cx));
    cx.assert_editor_state(after);
}

#[gpui::test]
async fn test_custom_fallback_highlights(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"fn main(self, variable: TType) {ˇ}"});

    let variable_color = Hsla::green();
    let function_color = Hsla::blue();

    let test_cases = [
        ("@variable", Some(variable_color)),
        ("@type", None),
        ("@type @variable", Some(variable_color)),
        ("@variable @type", Some(variable_color)),
        ("@variable @function", Some(function_color)),
        ("@function @variable", Some(variable_color)),
    ];

    for (test_case, expected) in test_cases {
        let custom_rust_lang = Arc::into_inner(rust_lang())
            .unwrap()
            .with_highlights_query(format! {r#"(type_identifier) {test_case}"#}.as_str())
            .unwrap();
        let theme = setup_syntax_highlighting(Arc::new(custom_rust_lang), &mut cx);
        let expected = expected.map_or_else(Vec::new, |expected_color| {
            vec![(24..29, HighlightStyle::color(expected_color))]
        });

        cx.update_editor(|editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            assert_eq!(
                expected,
                snapshot.combined_highlights(MultiBufferOffset(0)..snapshot.buffer().len(), &theme),
                "Test case with '{test_case}' highlights query did not pass",
            );
        });
    }
}

#[gpui::test]
async fn test_tsx_nested_jsx_member_expression_highlights(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state("<A.B.C></A.B.C>ˇ;");

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "TSX".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["tsx".to_string()],
                    ..LanguageMatcher::default()
                },
                ..LanguageConfig::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        )
        .with_highlights_query(include_str!("../../grammars/src/tsx/highlights.scm"))
        .unwrap(),
    );

    let component_color = Hsla::green();
    let theme = Arc::new(SyntaxTheme::new_test(vec![
        ("tag.component.jsx", component_color),
        ("type", Hsla::blue()),
        ("property", Hsla::red()),
        ("punctuation.bracket", Hsla::default()),
        ("punctuation.delimiter", Hsla::default()),
    ]));
    setup_syntax_highlighting_with_theme(language, theme.clone(), &mut cx);
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        assert_eq!(
            snapshot
                .combined_highlights(MultiBufferOffset(0)..snapshot.buffer().len(), &theme)
                .iter()
                .filter(|(_, style)| *style == HighlightStyle::color(component_color))
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                (1..2, HighlightStyle::color(component_color)),
                (3..4, HighlightStyle::color(component_color)),
                (5..6, HighlightStyle::color(component_color)),
                (9..10, HighlightStyle::color(component_color)),
                (11..12, HighlightStyle::color(component_color)),
                (13..14, HighlightStyle::color(component_color)),
            ],
        );
    });
}

fn setup_syntax_highlighting(
    language: Arc<Language>,
    cx: &mut EditorTestContext,
) -> Arc<SyntaxTheme> {
    let syntax = Arc::new(SyntaxTheme::new_test(vec![
        ("keyword", Hsla::red()),
        ("function", Hsla::blue()),
        ("variable", Hsla::green()),
        ("number", Hsla::default()),
        ("operator", Hsla::default()),
        ("punctuation.bracket", Hsla::default()),
        ("punctuation.delimiter", Hsla::default()),
    ]));

    setup_syntax_highlighting_with_theme(language, syntax.clone(), cx);
    syntax
}

fn setup_syntax_highlighting_with_theme(
    language: Arc<Language>,
    syntax: Arc<SyntaxTheme>,
    cx: &mut EditorTestContext,
) {
    language.set_theme(&syntax);

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.set_style(
            EditorStyle {
                syntax,
                ..EditorStyle::default()
            },
            window,
            cx,
        );
    });
}

#[gpui::test]
async fn test_toggle_diagnostics_persists_across_settings_change(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.update_editor(|editor, _, _| {
        assert!(
            editor.diagnostics_enabled(),
            "diagnostics should start enabled by default"
        );
    });

    cx.update_editor(|editor, window, cx| {
        editor.toggle_diagnostics(&actions::ToggleDiagnostics, window, cx);
        assert!(
            !editor.diagnostics_enabled(),
            "diagnostics should be disabled after toggle"
        );
    });

    update_test_editor_settings(&mut cx, &|settings| {
        settings.cursor_blink = Some(false);
    });
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        assert!(
            !editor.diagnostics_enabled(),
            "diagnostics should remain disabled after settings change"
        );
    });

    cx.update_editor(|editor, window, cx| {
        editor.toggle_diagnostics(&actions::ToggleDiagnostics, window, cx);
        assert!(
            editor.diagnostics_enabled(),
            "diagnostics should be re-enabled after second toggle"
        );
    });
}

#[gpui::test]
async fn test_columnar_selection_with_multibyte_chars(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // The middle row contains a 2-byte char (ã) before the dragged column. A
    // column selection that uses byte columns directly puts the ã row's
    // selection at a different visual position than the ASCII rows; anchoring
    // in x pixels keeps all rows at the same character offset.
    cx.set_state(indoc! {"
        ˇabcde
        abcde
        aãcde
        abcde
        abcde
    "});

    // Drag column-wise from (row 0, col 0) past the ã column on every row.
    cx.update_editor(|editor, window, cx| {
        editor.select(
            SelectPhase::BeginColumnar {
                position: DisplayPoint::new(DisplayRow(0), 0),
                goal_column: 0,
                reset: true,
                mode: ColumnarMode::FromMouse,
            },
            window,
            cx,
        );
        editor.select(
            SelectPhase::Update {
                position: DisplayPoint::new(DisplayRow(4), 4),
                goal_column: 4,
                scroll_delta: gpui::Point::default(),
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        «abcdˇ»e
        «abcdˇ»e
        «aãcdˇ»e
        «abcdˇ»e
        «abcdˇ»e
    "});

    // Control: drag stops before the ã column, where byte columns and x
    // positions agree.
    cx.update_editor(|editor, window, cx| {
        editor.select(
            SelectPhase::BeginColumnar {
                position: DisplayPoint::new(DisplayRow(0), 0),
                goal_column: 0,
                reset: true,
                mode: ColumnarMode::FromMouse,
            },
            window,
            cx,
        );
        editor.select(
            SelectPhase::Update {
                position: DisplayPoint::new(DisplayRow(4), 1),
                goal_column: 1,
                scroll_delta: gpui::Point::default(),
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        «aˇ»bcde
        «aˇ»bcde
        «aˇ»ãcde
        «aˇ»bcde
        «aˇ»bcde
    "});
}

#[gpui::test]
async fn test_columnar_selection_past_end_of_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        ˇaaaaaaaaaa
        bb
        cccccccccc
    "});

    // Drag from the start of the long first row to a point past the EOL of
    // the short second row: the mouse handlers encode that as the nearest
    // valid position (1, 2) plus an unclipped goal column of 8. The rectangle
    // must keep tracking the mouse x on the long row instead of collapsing to
    // the short row's width.
    cx.update_editor(|editor, window, cx| {
        editor.select(
            SelectPhase::BeginColumnar {
                position: DisplayPoint::new(DisplayRow(0), 0),
                goal_column: 0,
                reset: true,
                mode: ColumnarMode::FromMouse,
            },
            window,
            cx,
        );
        editor.select(
            SelectPhase::Update {
                position: DisplayPoint::new(DisplayRow(1), 2),
                goal_column: 8,
                scroll_delta: gpui::Point::default(),
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        «aaaaaaaaˇ»aa
        «bbˇ»
        cccccccccc
    "});

    // Starting the drag past the EOL of the short row must anchor that edge
    // of the rectangle at the click position, not at the short row's EOL.
    cx.update_editor(|editor, window, cx| {
        editor.select(
            SelectPhase::BeginColumnar {
                position: DisplayPoint::new(DisplayRow(1), 2),
                goal_column: 8,
                reset: true,
                mode: ColumnarMode::FromMouse,
            },
            window,
            cx,
        );
        editor.select(
            SelectPhase::Update {
                position: DisplayPoint::new(DisplayRow(2), 4),
                goal_column: 4,
                scroll_delta: gpui::Point::default(),
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(indoc! {"
        aaaaaaaaaa
        bb
        cccc«ˇcccc»cc
    "});
}

#[gpui::test]
async fn test_toggle_markdown_block_quote(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // No-op with no language
    cx.set_state(indoc! {"
        «helloˇ» world
    "});
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «helloˇ» world
    "});

    // No-op in non-Markdown language (Rust)
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));
    cx.set_state(indoc! {"
        «helloˇ» world
    "});
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «helloˇ» world
    "});

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_lang()), cx));

    // Line is quoted with an empty selection
    cx.set_state(indoc! {"
        helˇlo world
    "});
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «> hello worldˇ»
    "});

    // Line is unquoted with an empty selection
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «hello worldˇ»
    "});

    // Multi-line selection is quoted, including blank lines
    cx.set_state(indoc! {"
        «first

        thirdˇ»
    "});
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «> first
        >
        > thirdˇ»
    "});

    // Multi-line selection is unquoted, including blank lines
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «first

        thirdˇ»
    "});

    // A multi-line selection, including a mixture of quoted and unquoted lines
    // and a mixture of empty and non-empty lines, normalizes each line to a
    // single quote.
    cx.set_state(indoc! {"
        «> first
        second
        >

        > third
        >fourthˇ»
    "});
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «> first
        > second
        >
        >
        > third
        > fourthˇ»
    "});

    // A multi-line selection is unquoted.
    cx.update_editor(|e, window, cx| e.toggle_markdown_block_quote(&ToggleBlockQuote, window, cx));
    cx.assert_editor_state(indoc! {"
        «first
        second


        third
        fourthˇ»
    "});
}

#[track_caller]
fn assert_select_delimiters(around: bool, before: &str, after: &str, cx: &mut EditorTestContext) {
    let _state_context = cx.set_state(before);

    if around {
        cx.dispatch_action(SelectAroundDelimiters);
    } else {
        cx.dispatch_action(SelectInsideDelimiters);
    }

    cx.assert_editor_state(after);
}

#[gpui::test]
async fn test_select_delimiters(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_typescript(Default::default(), cx).await;

    // Inside.
    assert_select_delimiters(false, "foo(ˇbar);", "foo(«barˇ»);", &mut cx);
    assert_select_delimiters(false, "foo(a, ˇb, c);", "foo(«a, b, cˇ»);", &mut cx);
    assert_select_delimiters(false, "foo([1, ˇ2, 3]);", "foo([«1, 2, 3ˇ»]);", &mut cx);
    assert_select_delimiters(false, "let x = { aˇ: 1 };", "let x = {« a: 1 ˇ»};", &mut cx);
    assert_select_delimiters(false, "let xˇ = 42;", "let xˇ = 42;", &mut cx);
    assert_select_delimiters(false, "foo(a, «bˇ», c);", "foo(«a, b, cˇ»);", &mut cx);
    assert_select_delimiters(
        false,
        "const s = \"hello ˇworld\";",
        "const s = \"«hello worldˇ»\";",
        &mut cx,
    );

    assert_select_delimiters(
        false,
        "const s = \"ˇhello world\";",
        "const s = \"«hello worldˇ»\";",
        &mut cx,
    );

    assert_select_delimiters(
        false,
        "const s = \"hello worldˇ\";",
        "const s = \"«hello worldˇ»\";",
        &mut cx,
    );

    assert_select_delimiters(
        false,
        "console.log(\"deˇbug\");",
        "console.log(\"«debugˇ»\");",
        &mut cx,
    );

    // Around.
    assert_select_delimiters(true, "foo(ˇbar);", "foo«(bar)ˇ»;", &mut cx);
    assert_select_delimiters(true, "foo([1, ˇ2, 3]);", "foo(«[1, 2, 3]ˇ»);", &mut cx);
    assert_select_delimiters(true, "let x = {ˇ a: 1 };", "let x = «{ a: 1 }ˇ»;", &mut cx);
    assert_select_delimiters(true, "let xˇ = 42;", "let xˇ = 42;", &mut cx);
    assert_select_delimiters(
        true,
        "console.log(\"deˇbug\");",
        "console.log(«\"debug\"ˇ»);",
        &mut cx,
    );
}

#[gpui::test]
async fn test_select_delimiters_in_markdown(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_lang()), cx));

    // Inside.
    assert_select_delimiters(
        false,
        r#"This is "ˇhello, world!"."#,
        r#"This is "«hello, world!ˇ»"."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is "hello, ˇworld!"."#,
        r#"This is "«hello, world!ˇ»"."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is "hello, world!ˇ"."#,
        r#"This is "«hello, world!ˇ»"."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is ˇ"hello, world!"."#,
        r#"This is "«hello, world!ˇ»"."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is "hello, world!"ˇ."#,
        r#"This is "«hello, world!ˇ»"."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is 'hello, ˇworld!'."#,
        r#"This is '«hello, world!ˇ»'."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is `hello, ˇworld!`."#,
        r#"This is `«hello, world!ˇ»`."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is ("hello, ˇworld!")."#,
        r#"This is ("«hello, world!ˇ»")."#,
        &mut cx,
    );
    assert_select_delimiters(
        false,
        r#"This is hello, ˇworld!."#,
        r#"This is hello, ˇworld!."#,
        &mut cx,
    );

    // Around.
    assert_select_delimiters(
        true,
        r#"This is "hello, ˇworld!"."#,
        r#"This is «"hello, world!"ˇ»."#,
        &mut cx,
    );
    assert_select_delimiters(
        true,
        r#"This is 'hello, ˇworld!'."#,
        r#"This is «'hello, world!'ˇ»."#,
        &mut cx,
    );
    assert_select_delimiters(
        true,
        r#"This is `hello, ˇworld!`."#,
        r#"This is «`hello, world!`ˇ»."#,
        &mut cx,
    );
}

#[gpui::test]
async fn test_select_delimiters_expansion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_typescript(Default::default(), cx).await;

    let _state_context = cx.set_state("foo([1, ˇ2, 3]);");
    cx.dispatch_action(SelectInsideDelimiters);
    cx.assert_editor_state("foo([«1, 2, 3ˇ»]);");
    cx.dispatch_action(SelectInsideDelimiters);
    cx.assert_editor_state("foo(«[1, 2, 3]ˇ»);");

    let _state_context = cx.set_state("foo([1, ˇ2, 3]);");
    cx.dispatch_action(SelectInsideDelimiters);
    cx.assert_editor_state("foo([«1, 2, 3ˇ»]);");
    cx.dispatch_action(SelectAroundDelimiters);
    cx.assert_editor_state("foo(«[1, 2, 3]ˇ»);");
    cx.dispatch_action(SelectAroundDelimiters);
    cx.assert_editor_state("foo«([1, 2, 3])ˇ»;");

    let _state_context = cx.set_state("foo(x, { ˇa: 1 });");
    cx.dispatch_action(SelectInsideDelimiters);
    cx.assert_editor_state("foo(x, {« a: 1 ˇ»});");
    cx.dispatch_action(SelectAroundDelimiters);
    cx.assert_editor_state("foo(x, «{ a: 1 }ˇ»);");
    cx.dispatch_action(SelectInsideDelimiters);
    cx.assert_editor_state("foo(«x, { a: 1 }ˇ»);");
}

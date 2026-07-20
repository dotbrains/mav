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
#[path = "editor_tests/lsp_formatting_restart.rs"]
mod lsp_formatting_restart;
#[path = "editor_tests/manual_formatting.rs"]
mod manual_formatting;
#[path = "editor_tests/markdown_paste.rs"]
mod markdown_paste;
#[path = "editor_tests/mixed_completion_snippet.rs"]
mod mixed_completion_snippet;
#[path = "editor_tests/modal_popovers.rs"]
mod modal_popovers;
#[path = "editor_tests/multi_formatter.rs"]
mod multi_formatter;
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

#[gpui::test]
async fn test_no_duplicated_sticky_headers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        ˇimpl Foo { fn bar() {
            let x = 1;
            fn baz() {
                let y = 2;
            }
        } }
    "});

    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    let mut sticky_headers = |offset: ScrollOffset| {
        cx.update_editor(|e, window, cx| {
            e.scroll(gpui::Point { x: 0., y: offset }, None, window, cx);
        });
        cx.run_until_parked();
        cx.update_editor(|e, window, cx| {
            EditorElement::sticky_headers(&e, &e.snapshot(window, cx))
                .into_iter()
                .map(
                    |StickyHeader {
                         start_point,
                         offset,
                         ..
                     }| { (start_point, offset) },
                )
                .collect::<Vec<_>>()
        })
    };

    let struct_foo = Point { row: 0, column: 0 };
    let fn_baz = Point { row: 2, column: 4 };

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(0.5), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(1.0), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(1.5), vec![(struct_foo, 0.0), (fn_baz, 1.0)]);
    assert_eq!(sticky_headers(2.0), vec![(struct_foo, 0.0), (fn_baz, 1.0)]);
    assert_eq!(sticky_headers(2.5), vec![(struct_foo, 0.0), (fn_baz, 0.5)]);
    assert_eq!(sticky_headers(3.0), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(3.5), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(4.0), vec![(struct_foo, 0.0)]);
    assert_eq!(sticky_headers(4.5), vec![(struct_foo, -0.5)]);
    assert_eq!(sticky_headers(5.0), vec![]);
}

#[gpui::test]
async fn test_autoscroll_keeps_cursor_visible_below_sticky_headers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.vertical_scroll_margin = Some(0.0);
        settings.scroll_beyond_last_line = Some(ScrollBeyondLastLine::OnePage);
        settings.sticky_scroll = Some(settings::StickyScrollContent {
            enabled: Some(true),
        });
    });
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        impl Foo { fn bar() {
            let x = 1;
            fn baz() {
                let y = 2;
            }
        } }
        ˇ
    "});

    let mut previous_cursor_row = cx.update_editor(|editor, window, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| buffer.set_language(Some(rust_lang()), cx));
        let cursor_row = editor
            .selections
            .newest_display(&editor.display_snapshot(cx))
            .head()
            .row();
        editor.set_scroll_top_row(cursor_row, window, cx);
        cursor_row
    });

    for _ in 0..6 {
        cx.update_editor(|editor, window, cx| editor.move_up(&MoveUp, window, cx));
        cx.run_until_parked();

        cx.update_editor(|editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            let scroll_top = snapshot.scroll_position().y;
            let sticky_header_count = EditorElement::sticky_headers(editor, &snapshot).len();
            let cursor_row = editor
                .selections
                .newest_display(&snapshot.display_snapshot)
                .head()
                .row();
            assert_eq!(
                cursor_row,
                previous_cursor_row
                    .previous_row()
                    .max(DisplayRow(scroll_top as u32) + DisplayRow(sticky_header_count as u32))
            );
            previous_cursor_row = cursor_row;
        });

        // The `ScrollCursorTop` action shouldn't change the scroll position, as the cursor is
        // already as high up as the sticky headers allow.
        let scroll_top_before =
            cx.update_editor(|editor, window, cx| editor.snapshot(window, cx).scroll_position().y);
        cx.update_editor(|editor, window, cx| {
            editor.scroll_cursor_top(&ScrollCursorTop, window, cx)
        });
        cx.run_until_parked();
        let scroll_top_after =
            cx.update_editor(|editor, window, cx| editor.snapshot(window, cx).scroll_position().y);
        assert_eq!(scroll_top_before, scroll_top_after);
    }
}

#[gpui::test]
fn test_relative_line_numbers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer_1 = cx.new(|cx| Buffer::local("aaaaaaaaaa\nbbb\n", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("cccccccccc\nddd\n", cx));
    let buffer_3 = cx.new(|cx| Buffer::local("eee\nffffffffff\n", cx));

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [Point::new(0, 0)..Point::new(2, 0)],
            0,
            cx,
        );
        multibuffer
    });

    // wrapped contents of multibuffer:
    //    aaa
    //    aaa
    //    aaa
    //    a
    //    bbb
    //
    //    ccc
    //    ccc
    //    ccc
    //    c
    //    ddd
    //
    //    eee
    //    fff
    //    fff
    //    fff
    //    f

    let editor = cx.add_window(|window, cx| build_editor(multibuffer, window, cx));
    _ = editor.update(cx, |editor, window, cx| {
        editor.set_wrap_width(Some(30.0.into()), cx); // every 3 characters

        // includes trailing newlines.
        let expected_line_numbers = [2, 6, 7, 10, 14, 15, 18, 19, 23];
        let expected_wrapped_line_numbers = [
            2, 3, 4, 5, 6, 7, 10, 11, 12, 13, 14, 15, 18, 19, 20, 21, 22, 23,
        ];

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                Point::new(7, 0)..Point::new(7, 1), // second row of `ccc`
            ]);
        });

        let snapshot = editor.snapshot(window, cx);

        // these are all 0-indexed
        let base_display_row = DisplayRow(11);
        let base_row = 3;
        let wrapped_base_row = 7;

        // test not counting wrapped lines
        let expected_relative_numbers = expected_line_numbers
            .into_iter()
            .enumerate()
            .map(|(i, row)| (DisplayRow(row), i.abs_diff(base_row) as u32))
            .filter(|(_, relative_line_number)| *relative_line_number != 0)
            .collect_vec();
        let actual_relative_numbers = snapshot
            .calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(24)),
                base_display_row,
                false,
            )
            .into_iter()
            .sorted()
            .collect_vec();
        assert_eq!(expected_relative_numbers, actual_relative_numbers);
        // check `calculate_relative_line_numbers()` against `relative_line_delta()` for each line
        for (display_row, relative_number) in expected_relative_numbers {
            assert_eq!(
                relative_number,
                snapshot
                    .relative_line_delta(display_row, base_display_row, false)
                    .unsigned_abs() as u32,
            );
        }

        // test counting wrapped lines
        let expected_wrapped_relative_numbers = expected_wrapped_line_numbers
            .into_iter()
            .enumerate()
            .map(|(i, row)| (DisplayRow(row), i.abs_diff(wrapped_base_row) as u32))
            .filter(|(row, _)| *row != base_display_row)
            .collect_vec();
        let actual_relative_numbers = snapshot
            .calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(24)),
                base_display_row,
                true,
            )
            .into_iter()
            .sorted()
            .collect_vec();
        assert_eq!(expected_wrapped_relative_numbers, actual_relative_numbers);
        // check `calculate_relative_line_numbers()` against `relative_wrapped_line_delta()` for each line
        for (display_row, relative_number) in expected_wrapped_relative_numbers {
            assert_eq!(
                relative_number,
                snapshot
                    .relative_line_delta(display_row, base_display_row, true)
                    .unsigned_abs() as u32,
            );
        }
    });
}

#[gpui::test]
async fn test_scroll_by_clicking_sticky_header(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.sticky_scroll = Some(settings::StickyScrollContent {
                    enabled: Some(true),
                })
            });
        });
    });
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });

    let buffer = indoc! {"
            ˇfn foo() {
                let abc = 123;
            }
            struct Bar;
            impl Bar {
                fn new() -> Self {
                    Self
                }
            }
            fn baz() {
            }
        "};
    cx.set_state(&buffer);

    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    let fn_foo = || empty_range(0, 0);
    let impl_bar = || empty_range(4, 0);
    let fn_new = || empty_range(5, 0);

    let mut scroll_and_click = |scroll_offset: ScrollOffset, click_offset: ScrollOffset| {
        cx.update_editor(|e, window, cx| {
            e.scroll(
                gpui::Point {
                    x: 0.,
                    y: scroll_offset,
                },
                None,
                window,
                cx,
            );
        });
        cx.run_until_parked();
        cx.simulate_click(
            gpui::Point {
                x: px(0.),
                y: click_offset as f32 * line_height,
            },
            Modifiers::none(),
        );
        cx.run_until_parked();
        cx.update_editor(|e, _, cx| (e.scroll_position(cx), display_ranges(e, cx)))
    };
    assert_eq!(
        scroll_and_click(
            4.5, // impl Bar is halfway off the screen
            0.0  // click top of screen
        ),
        // scrolled to impl Bar
        (gpui::Point { x: 0., y: 4. }, vec![impl_bar()])
    );

    assert_eq!(
        scroll_and_click(
            4.5,  // impl Bar is halfway off the screen
            0.25  // click middle of impl Bar
        ),
        // scrolled to impl Bar
        (gpui::Point { x: 0., y: 4. }, vec![impl_bar()])
    );

    assert_eq!(
        scroll_and_click(
            4.5, // impl Bar is halfway off the screen
            1.5  // click below impl Bar (e.g. fn new())
        ),
        // scrolled to fn new() - this is below the impl Bar header which has persisted
        (gpui::Point { x: 0., y: 4. }, vec![fn_new()])
    );

    assert_eq!(
        scroll_and_click(
            5.5,  // fn new is halfway underneath impl Bar
            0.75  // click on the overlap of impl Bar and fn new()
        ),
        (gpui::Point { x: 0., y: 4. }, vec![impl_bar()])
    );

    assert_eq!(
        scroll_and_click(
            5.5,  // fn new is halfway underneath impl Bar
            1.25  // click on the visible part of fn new()
        ),
        (gpui::Point { x: 0., y: 4. }, vec![fn_new()])
    );

    assert_eq!(
        scroll_and_click(
            1.5, // fn foo is halfway off the screen
            0.0  // click top of screen
        ),
        (gpui::Point { x: 0., y: 0. }, vec![fn_foo()])
    );

    assert_eq!(
        scroll_and_click(
            1.5,  // fn foo is halfway off the screen
            0.75  // click visible part of let abc...
        )
        .0,
        // no change in scroll
        // we don't assert on the visible_range because if we clicked the gutter, our line is fully selected
        (gpui::Point { x: 0., y: 1.5 })
    );

    // Verify clicking at a specific x position within a sticky header places
    // the cursor at the corresponding column.
    let (text_origin_x, em_width) = cx.update_editor(|editor, _, _| {
        let position_map = editor.last_position_map.as_ref().unwrap();
        (
            position_map.text_hitbox.bounds.origin.x,
            position_map.em_layout_width,
        )
    });

    // Click on "impl Bar {" sticky header at column 5 (the 'B' in 'Bar').
    // The text "impl Bar {" starts at column 0, so column 5 = 'B'.
    let click_x = text_origin_x + em_width * 5.5;
    cx.update_editor(|e, window, cx| {
        e.scroll(gpui::Point { x: 0., y: 4.5 }, None, window, cx);
    });
    cx.run_until_parked();
    cx.simulate_click(
        gpui::Point {
            x: click_x,
            y: 0.25 * line_height,
        },
        Modifiers::none(),
    );
    cx.run_until_parked();
    let (scroll_pos, selections) =
        cx.update_editor(|e, _, cx| (e.scroll_position(cx), display_ranges(e, cx)));
    assert_eq!(scroll_pos, gpui::Point { x: 0., y: 4. });
    assert_eq!(selections, vec![empty_range(4, 5)]);
}

#[gpui::test]
async fn test_clicking_sticky_header_sets_character_select_mode(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.editor.sticky_scroll = Some(settings::StickyScrollContent {
                    enabled: Some(true),
                })
            });
        });
    });
    let mut cx = EditorTestContext::new(cx).await;

    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });

    let buffer = indoc! {"
            fn foo() {
                let abc = 123;
            }
            ˇstruct Bar;
        "};
    cx.set_state(&buffer);

    cx.update_editor(|editor, _, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(rust_lang()), cx);
            })
    });

    let text_origin_x = cx.update_editor(|editor, _, _| {
        editor
            .last_position_map
            .as_ref()
            .unwrap()
            .text_hitbox
            .bounds
            .origin
            .x
    });

    cx.update_editor(|editor, window, cx| {
        // Double click on `struct` to select it
        editor.begin_selection(DisplayPoint::new(DisplayRow(3), 1), false, 2, window, cx);
        editor.end_selection(window, cx);

        // Scroll down one row to make `fn foo() {` a sticky header
        editor.scroll(gpui::Point { x: 0., y: 1. }, None, window, cx);
    });
    cx.run_until_parked();

    // Click at the start of the `fn foo() {` sticky header
    cx.simulate_click(
        gpui::Point {
            x: text_origin_x,
            y: 0.5 * line_height,
        },
        Modifiers::none(),
    );
    cx.run_until_parked();

    // Shift-click at the end of `fn foo() {` to select the whole row
    cx.update_editor(|editor, window, cx| {
        editor.extend_selection(DisplayPoint::new(DisplayRow(0), 10), 1, window, cx);
        editor.end_selection(window, cx);
    });
    cx.run_until_parked();

    let selections = cx.update_editor(|editor, _, cx| display_ranges(editor, cx));
    assert_eq!(
        selections,
        vec![DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 10)]
    );
}

#[gpui::test]
async fn test_next_prev_reference(cx: &mut TestAppContext) {
    const CYCLE_POSITIONS: &[&'static str] = &[
        indoc! {"
            fn foo() {
                let ˇabc = 123;
                let x = abc + 1;
                let y = abc + 2;
                let z = abc + 2;
            }
        "},
        indoc! {"
            fn foo() {
                let abc = 123;
                let x = ˇabc + 1;
                let y = abc + 2;
                let z = abc + 2;
            }
        "},
        indoc! {"
            fn foo() {
                let abc = 123;
                let x = abc + 1;
                let y = ˇabc + 2;
                let z = abc + 2;
            }
        "},
        indoc! {"
            fn foo() {
                let abc = 123;
                let x = abc + 1;
                let y = abc + 2;
                let z = ˇabc + 2;
            }
        "},
    ];

    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            references_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // importantly, the cursor is in the middle
    cx.set_state(indoc! {"
        fn foo() {
            let aˇbc = 123;
            let x = abc + 1;
            let y = abc + 2;
            let z = abc + 2;
        }
    "});

    let reference_ranges = [
        lsp::Position::new(1, 8),
        lsp::Position::new(2, 12),
        lsp::Position::new(3, 12),
        lsp::Position::new(4, 12),
    ]
    .map(|start| lsp::Range::new(start, lsp::Position::new(start.line, start.character + 3)));

    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(move |params, _cx| async move {
            Ok(Some(
                reference_ranges
                    .map(|range| lsp::Location {
                        uri: params.text_document_position.text_document.uri.clone(),
                        range,
                    })
                    .to_vec(),
            ))
        });

    let _move = async |direction, count, cx: &mut EditorLspTestContext| {
        cx.update_editor(|editor, window, cx| {
            editor.go_to_reference_before_or_after_position(direction, count, window, cx)
        })
        .unwrap()
        .await
        .unwrap()
    };

    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[1]);

    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[2]);

    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[3]);

    // loops back to the start
    _move(Direction::Next, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[0]);

    // loops back to the end
    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[3]);

    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[2]);

    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[1]);

    _move(Direction::Prev, 1, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[0]);

    _move(Direction::Next, 3, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[3]);

    _move(Direction::Prev, 2, &mut cx).await;
    cx.assert_editor_state(CYCLE_POSITIONS[1]);
}

#[gpui::test]
async fn test_multibuffer_selections_with_folding(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("1\n2\n3\n", vec![Point::row_range(0..3)]),
                ("1\n2\n3\n", vec![Point::row_range(0..3)]),
            ],
            cx,
        );
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;
    let buffer_ids = cx.multibuffer(|mb, cx| {
        mb.snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>()
    });

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ1
        2
        3
        [EXCERPT]
        1
        2
        3
        "});

    // Scenario 1: Unfolded buffers, position cursor on "2", select all matches, then insert
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(3)]);
        });
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2ˇ
        3
        [EXCERPT]
        1
        2
        3
        "});

    cx.update_editor(|editor, window, cx| {
        editor
            .select_all_matches(&SelectAllMatches, window, cx)
            .unwrap();
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2ˇ
        3
        [EXCERPT]
        1
        2ˇ
        3
        "});

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("X", window, cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        Xˇ
        3
        [EXCERPT]
        1
        Xˇ
        3
        "});

    // Scenario 2: Select "2", then fold second buffer before insertion
    cx.update_multibuffer(|mb, cx| {
        for buffer_id in buffer_ids.iter() {
            let buffer = mb.buffer(*buffer_id).unwrap();
            buffer.update(cx, |buffer, cx| {
                buffer.edit([(0..buffer.len(), "1\n2\n3\n")], None, cx);
            });
        }
    });

    // Select "2" and select all matches
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(3)]);
        });
        editor
            .select_all_matches(&SelectAllMatches, window, cx)
            .unwrap();
    });

    // Fold second buffer - should remove selections from folded buffer
    cx.update_editor(|editor, _, cx| {
        editor.fold_buffer(buffer_ids[1], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2ˇ
        3
        [EXCERPT]
        [FOLDED]
        "});

    // Insert text - should only affect first buffer
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Y", window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        editor.unfold_buffer(buffer_ids[1], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        Yˇ
        3
        [EXCERPT]
        1
        2
        3
        "});

    // Scenario 3: Select "2", then fold first buffer before insertion
    cx.update_multibuffer(|mb, cx| {
        for buffer_id in buffer_ids.iter() {
            let buffer = mb.buffer(*buffer_id).unwrap();
            buffer.update(cx, |buffer, cx| {
                buffer.edit([(0..buffer.len(), "1\n2\n3\n")], None, cx);
            });
        }
    });

    // Select "2" and select all matches
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(3)]);
        });
        editor
            .select_all_matches(&SelectAllMatches, window, cx)
            .unwrap();
    });

    // Fold first buffer - should remove selections from folded buffer
    cx.update_editor(|editor, _, cx| {
        editor.fold_buffer(buffer_ids[0], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        1
        2ˇ
        3
        "});

    // Insert text - should only affect second buffer
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Z", window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        editor.unfold_buffer(buffer_ids[0], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2
        3
        [EXCERPT]
        1
        Zˇ
        3
        "});

    // Test correct folded header is selected upon fold
    cx.update_editor(|editor, _, cx| {
        editor.fold_buffer(buffer_ids[0], cx);
        editor.fold_buffer(buffer_ids[1], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇ[FOLDED]
        "});

    // Test selection inside folded buffer unfolds it on type
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("W", window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        editor.unfold_buffer(buffer_ids[0], cx);
    });
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2
        3
        [EXCERPT]
        Wˇ1
        Z
        3
        "});
}

#[gpui::test]
async fn test_multibuffer_scroll_cursor_top_margin(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("1\n2\n3\n", vec![Point::row_range(0..3)]),
                ("1\n2\n3\n4\n5\n6\n7\n8\n9\n", vec![Point::row_range(0..9)]),
            ],
            cx,
        );
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ1
        2
        3
        [EXCERPT]
        1
        2
        3
        4
        5
        6
        7
        8
        9
        "});

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(19)..MultiBufferOffset(19)]);
        });
    });

    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        1
        2
        3
        [EXCERPT]
        1
        2
        3
        4
        5
        6
        ˇ7
        8
        9
        "});

    cx.update_editor(|editor, _window, cx| {
        editor.set_vertical_scroll_margin(0, cx);
    });

    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.vertical_scroll_margin(), 0);
        editor.scroll_cursor_top(&ScrollCursorTop, window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 12.0)
        );
    });

    cx.update_editor(|editor, _window, cx| {
        editor.set_vertical_scroll_margin(3, cx);
    });

    cx.update_editor(|editor, window, cx| {
        assert_eq!(editor.vertical_scroll_margin(), 3);
        editor.scroll_cursor_top(&ScrollCursorTop, window, cx);
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0., 9.0)
        );
    });
}

#[gpui::test]
async fn test_find_references_single_case(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let before = indoc!(
        r#"
        fn main() {
            let aˇbc = 123;
            let xyz = abc;
        }
        "#
    );
    let after = indoc!(
        r#"
        fn main() {
            let abc = 123;
            let xyz = ˇabc;
        }
        "#
    );

    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(async move |params, _| {
            Ok(Some(vec![
                lsp::Location {
                    uri: params.text_document_position.text_document.uri.clone(),
                    range: lsp::Range::new(lsp::Position::new(1, 8), lsp::Position::new(1, 11)),
                },
                lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(2, 14), lsp::Position::new(2, 17)),
                },
            ]))
        });

    cx.set_state(before);

    let action = FindAllReferences {
        always_open_multibuffer: false,
    };

    let navigated = cx
        .update_editor(|editor, window, cx| editor.find_all_references(&action, window, cx))
        .expect("should have spawned a task")
        .await
        .unwrap();

    assert_eq!(navigated, Navigated::No);

    cx.run_until_parked();

    cx.assert_editor_state(after);
}

#[gpui::test]
async fn test_newline_task_list_continuation(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Adding newline after (whitespace + prefix + any non-whitespace) adds marker
    cx.set_state(indoc! {"
        - [ ] taskˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
        - [ ] ˇ
    "});

    // Case 2: Works with checked task items too
    cx.set_state(indoc! {"
        - [x] completed taskˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [x] completed task
        - [ ] ˇ
    "});

    // Case 2.1: Works with uppercase checked marker too
    cx.set_state(indoc! {"
        - [X] completed taskˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [X] completed task
        - [ ] ˇ
    "});

    // Case 3: Cursor position doesn't matter - content after marker is what counts
    cx.set_state(indoc! {"
        - [ ] taˇsk
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] ta
        - [ ] ˇsk
    "});

    // Case 4: Adding newline after (whitespace + prefix + some whitespace) does NOT add marker
    cx.set_state(indoc! {"
        - [ ]  ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        - [ ]$$
        ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );

    // Case 5: Adding newline with content adds marker preserving indentation
    cx.set_state(indoc! {"
        - [ ] task
          - [ ] indentedˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] indented
          - [ ] ˇ
    "});

    // Case 6: Adding newline with cursor right after prefix, unindents
    cx.set_state(indoc! {"
        - [ ] task
          - [ ] sub task
            - [ ] ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] sub task
          - [ ] ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;

    // Case 7: Adding newline with cursor right after prefix, removes marker
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] sub task
        - [ ] ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [ ] task
          - [ ] sub task
        ˇ
    "});

    // Case 8: Cursor before or inside prefix does not add marker
    cx.set_state(indoc! {"
        ˇ- [ ] task
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        ˇ- [ ] task
    "});

    cx.set_state(indoc! {"
        - [ˇ ] task
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - [
        ˇ
        ] task
    "});
}

#[gpui::test]
async fn test_newline_unordered_list_continuation(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Adding newline after (whitespace + marker + any non-whitespace) adds marker
    cx.set_state(indoc! {"
        - itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
        - ˇ
    "});

    // Case 2: Works with different markers
    cx.set_state(indoc! {"
        * starred itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        * starred item
        * ˇ
    "});

    cx.set_state(indoc! {"
        + plus itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        + plus item
        + ˇ
    "});

    // Case 3: Cursor position doesn't matter - content after marker is what counts
    cx.set_state(indoc! {"
        - itˇem
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - it
        - ˇem
    "});

    // Case 4: Adding newline after (whitespace + marker + some whitespace) does NOT add marker
    cx.set_state(indoc! {"
        -  ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        - $
        ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );

    // Case 5: Adding newline with content adds marker preserving indentation
    cx.set_state(indoc! {"
        - item
          - indentedˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - indented
          - ˇ
    "});

    // Case 6: Adding newline with cursor right after marker, unindents
    cx.set_state(indoc! {"
        - item
          - sub item
            - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
          - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;

    // Case 7: Adding newline with cursor right after marker, removes marker
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
        - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
        ˇ
    "});

    // Case 8: Cursor before or inside prefix does not add marker
    cx.set_state(indoc! {"
        ˇ- item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        ˇ- item
    "});

    cx.set_state(indoc! {"
        -ˇ item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        -
        ˇitem
    "});

    update_test_language_settings(&mut cx, &|settings| {
        settings.defaults.tab_size = Some(4.try_into().unwrap());
    });

    // Case 9: Empty list item unindent works when tab size is larger than list indentation
    cx.set_state(indoc! {"
        - item
          - sub item
          - ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        - item
          - sub item
        - ˇ
    "});

    // Case 10: Empty list item unindent moves to the previous tab stop
    cx.set_state(
        indoc! {"
        $$$$$$- ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        $$$$- ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );
}

#[gpui::test]
async fn test_newline_ordered_list_continuation(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Adding newline after (whitespace + marker + any non-whitespace) increments number
    cx.set_state(indoc! {"
        1. first itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. first item
        2. ˇ
    "});

    // Case 2: Works with larger numbers
    cx.set_state(indoc! {"
        10. tenth itemˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        10. tenth item
        11. ˇ
    "});

    // Case 3: Cursor position doesn't matter - content after marker is what counts
    cx.set_state(indoc! {"
        1. itˇem
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. it
        2. ˇem
    "});

    // Case 4: Adding newline after (whitespace + marker + some whitespace) does NOT add marker
    cx.set_state(indoc! {"
        1.  ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(
        indoc! {"
        1. $
        ˇ
    "}
        .replace("$", " ")
        .as_str(),
    );

    // Case 5: Adding newline with content adds marker preserving indentation
    cx.set_state(indoc! {"
        1. item
          2. indentedˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. item
          2. indented
          3. ˇ
    "});

    // Case 6: Adding newline with cursor right after marker, unindents
    cx.set_state(indoc! {"
        1. item
          2. sub item
            3. ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. item
          2. sub item
          1. ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;

    // Case 7: Adding newline with cursor right after marker, removes marker
    cx.assert_editor_state(indoc! {"
        1. item
          2. sub item
        1. ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. item
          2. sub item
        ˇ
    "});

    // Case 8: Cursor before or inside prefix does not add marker
    cx.set_state(indoc! {"
        ˇ1. item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        ˇ1. item
    "});

    cx.set_state(indoc! {"
        1ˇ. item
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1
        ˇ. item
    "});
}

#[gpui::test]
async fn test_newline_should_not_autoindent_ordered_list(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Adding newline after (whitespace + marker + any non-whitespace) increments number
    cx.set_state(indoc! {"
        1. first item
          1. sub first item
          2. sub second item
          3. ˇ
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        1. first item
          1. sub first item
          2. sub second item
        1. ˇ
    "});
}

#[gpui::test]
async fn test_tab_list_indent(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Unordered list - cursor after prefix, adds indent before prefix
    cx.set_state(indoc! {"
        - ˇitem
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        $$- ˇitem
    "};
    cx.assert_editor_state(expected.replace("$", " ").as_str());

    // Case 2: Task list - cursor after prefix
    cx.set_state(indoc! {"
        - [ ] ˇtask
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        $$- [ ] ˇtask
    "};
    cx.assert_editor_state(expected.replace("$", " ").as_str());

    // Case 3: Ordered list - cursor after prefix
    cx.set_state(indoc! {"
        1. ˇfirst
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        $$1. ˇfirst
    "};
    cx.assert_editor_state(expected.replace("$", " ").as_str());

    // Case 4: With existing indentation - adds more indent
    let initial = indoc! {"
        $$- ˇitem
    "};
    cx.set_state(initial.replace("$", " ").as_str());
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        $$$$- ˇitem
    "};
    cx.assert_editor_state(expected.replace("$", " ").as_str());

    // Case 5: Empty list item
    cx.set_state(indoc! {"
        - ˇ
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        $$- ˇ
    "};
    cx.assert_editor_state(expected.replace("$", " ").as_str());

    // Case 6: Cursor at end of line with content
    cx.set_state(indoc! {"
        - itemˇ
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        $$- itemˇ
    "};
    cx.assert_editor_state(expected.replace("$", " ").as_str());

    // Case 7: Cursor at start of list item, indents it
    cx.set_state(indoc! {"
        - item
        ˇ  - sub item
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        - item
          ˇ  - sub item
    "};
    cx.assert_editor_state(expected);

    // Case 8: Cursor at start of list item, moves the cursor when "indent_list_on_tab" is false
    cx.update_editor(|_, _, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.indent_list_on_tab = Some(false);
            });
        });
    });
    cx.set_state(indoc! {"
        - item
        ˇ  - sub item
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    let expected = indoc! {"
        - item
          ˇ- sub item
    "};
    cx.assert_editor_state(expected);
}

#[gpui::test]
async fn test_local_worktree_trust(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| project::trusted_worktrees::init(HashMap::default(), cx));

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        ..InlayHintSettingsContent::default()
                    });
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".mav": {
                "settings.json": r#"{"languages":{"Rust":{"language_servers":["override-rust-analyzer"]}}}"#
            },
            "main.rs": "fn main() {}"
        }),
    )
    .await;

    let lsp_inlay_hint_request_count = Arc::new(AtomicUsize::new(0));
    let server_name = "override-rust-analyzer";
    let project = Project::test_with_worktree_trust(fs, [path!("/project").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let capabilities = lsp::ServerCapabilities {
        inlay_hint_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: server_name,
            capabilities,
            initializer: Some(Box::new({
                let lsp_inlay_hint_request_count = lsp_inlay_hint_request_count.clone();
                move |fake_server| {
                    let lsp_inlay_hint_request_count = lsp_inlay_hint_request_count.clone();
                    fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                        move |_params, _| {
                            lsp_inlay_hint_request_count.fetch_add(1, atomic::Ordering::Release);
                            async move {
                                Ok(Some(vec![lsp::InlayHint {
                                    position: lsp::Position::new(0, 0),
                                    label: lsp::InlayHintLabel::String("hint".to_string()),
                                    kind: None,
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                }]))
                            }
                        },
                    );
                }
            })),
            ..FakeLspAdapter::default()
        },
    );

    cx.run_until_parked();

    let worktree_id = project.read_with(cx, |project, cx| {
        project
            .worktrees(cx)
            .next()
            .map(|wt| wt.read(cx).id())
            .expect("should have a worktree")
    });
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());

    let trusted_worktrees =
        cx.update(|cx| TrustedWorktrees::try_get_global(cx).expect("trust global should exist"));

    let can_trust = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(!can_trust, "worktree should be restricted initially");

    let buffer_before_approval = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            cx.new(|cx| MultiBuffer::singleton(buffer_before_approval.clone(), cx)),
            Some(project.clone()),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let fake_language_server = fake_language_servers.next();

    cx.read(|cx| {
        assert_eq!(
            language::language_settings::LanguageSettings::for_buffer(
                buffer_before_approval.read(cx),
                cx
            )
            .language_servers,
            ["...".to_string()],
            "local .mav/settings.json must not apply before trust approval"
        )
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.handle_input("1", window, cx);
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    assert_eq!(
        lsp_inlay_hint_request_count.load(atomic::Ordering::Acquire),
        0,
        "inlay hints must not be queried before trust approval"
    );

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            std::collections::HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });
    cx.run_until_parked();

    cx.read(|cx| {
        assert_eq!(
            language::language_settings::LanguageSettings::for_buffer(
                buffer_before_approval.read(cx),
                cx
            )
            .language_servers,
            ["override-rust-analyzer".to_string()],
            "local .mav/settings.json should apply after trust approval"
        )
    });
    let _fake_language_server = fake_language_server.await.unwrap();
    editor.update_in(cx, |editor, window, cx| {
        editor.handle_input("1", window, cx);
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    assert!(
        lsp_inlay_hint_request_count.load(atomic::Ordering::Acquire) > 0,
        "inlay hints should be queried after trust approval"
    );

    let can_trust_after = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(can_trust_after, "worktree should be trusted after trust()");
}

#[gpui::test]
fn test_editor_rendering_when_positioned_above_viewport(cx: &mut TestAppContext) {
    // This test reproduces a bug where drawing an editor at a position above the viewport
    // (simulating what happens when an AutoHeight editor inside a List is scrolled past)
    // causes an infinite loop in blocks_in_range.
    //
    // The issue: when the editor's bounds.origin.y is very negative (above the viewport),
    // the content mask intersection produces visible_bounds with origin at the viewport top.
    // This makes clipped_top_in_lines very large, causing start_row to exceed max_row.
    // When blocks_in_range is called with start_row > max_row, the cursor seeks to the end
    // but the while loop after seek never terminates because cursor.next() is a no-op at end.
    init_test(cx, |_| {});

    let window = cx.add_window(|_, _| gpui::Empty);
    let mut cx = VisualTestContext::from_window(*window, cx);

    let buffer = cx.update(|_, cx| MultiBuffer::build_simple("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n", cx));
    let editor = cx.new_window_entity(|window, cx| build_editor(buffer, window, cx));

    // Simulate a small viewport (500x500 pixels at origin 0,0)
    cx.simulate_resize(gpui::size(px(500.), px(500.)));

    // Draw the editor at a very negative Y position, simulating an editor that's been
    // scrolled way above the visible viewport (like in a List that has scrolled past it).
    // The editor is 3000px tall but positioned at y=-10000, so it's entirely above the viewport.
    // This should NOT hang - it should just render nothing.
    cx.draw(
        gpui::point(px(0.), px(-10000.)),
        gpui::size(px(500.), px(3000.)),
        |_, _| editor.clone().into_any_element(),
    );

    // If we get here without hanging, the test passes
}

#[gpui::test]
async fn test_diff_review_indicator_created_on_gutter_hover(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({ "file.txt": "hello\nworld\n" }))
        .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/root/file.txt")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Enable diff review button mode
    editor.update(cx, |editor, cx| {
        editor.set_show_diff_review_button(true, cx);
    });

    // Initially, no indicator should be present
    editor.update(cx, |editor, _cx| {
        assert!(
            editor.gutter_diff_review_indicator.0.is_none(),
            "Indicator should be None initially"
        );
    });
}

#[gpui::test]
async fn test_diff_review_button_hidden_when_ai_disabled(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Register DisableAiSettings and set disable_ai to true
    cx.update(|cx| {
        project::DisableAiSettings::register(cx);
        project::DisableAiSettings::override_global(
            project::DisableAiSettings { disable_ai: true },
            cx,
        );
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({ "file.txt": "hello\nworld\n" }))
        .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/root/file.txt")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Enable diff review button mode
    editor.update(cx, |editor, cx| {
        editor.set_show_diff_review_button(true, cx);
    });

    // Verify AI is disabled
    cx.read(|cx| {
        assert!(
            project::DisableAiSettings::get_global(cx).disable_ai,
            "AI should be disabled"
        );
    });

    // The indicator should not be created when AI is disabled
    // (The mouse_moved handler checks DisableAiSettings before creating the indicator)
    editor.update(cx, |editor, _cx| {
        assert!(
            editor.gutter_diff_review_indicator.0.is_none(),
            "Indicator should be None when AI is disabled"
        );
    });
}

#[gpui::test]
async fn test_diff_review_button_shown_when_ai_enabled(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Register DisableAiSettings and set disable_ai to false
    cx.update(|cx| {
        project::DisableAiSettings::register(cx);
        project::DisableAiSettings::override_global(
            project::DisableAiSettings { disable_ai: false },
            cx,
        );
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({ "file.txt": "hello\nworld\n" }))
        .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/root/file.txt")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Enable diff review button mode
    editor.update(cx, |editor, cx| {
        editor.set_show_diff_review_button(true, cx);
    });

    // Verify AI is enabled
    cx.read(|cx| {
        assert!(
            !project::DisableAiSettings::get_global(cx).disable_ai,
            "AI should be enabled"
        );
    });

    // The show_diff_review_button flag should be true
    editor.update(cx, |editor, _cx| {
        assert!(
            editor.show_diff_review_button(),
            "show_diff_review_button should be true"
        );
    });
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
fn test_review_comment_add_to_hunk(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor: &mut Editor, _window, cx| {
        let key = test_hunk_key("");

        let id = add_test_comment(editor, key.clone(), "Test comment", cx);

        let snapshot = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(editor.total_review_comment_count(), 1);
        assert_eq!(editor.hunk_comment_count(&key, &snapshot), 1);

        let comments = editor.comments_for_hunk(&key, &snapshot);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].comment, "Test comment");
        assert_eq!(comments[0].id, id);
    });
}

#[gpui::test]
fn test_review_comments_are_per_hunk(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor: &mut Editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let anchor1 = snapshot.anchor_before(Point::new(0, 0));
        let anchor2 = snapshot.anchor_before(Point::new(0, 0));
        let key1 = test_hunk_key_with_anchor("file1.rs", anchor1);
        let key2 = test_hunk_key_with_anchor("file2.rs", anchor2);

        add_test_comment(editor, key1.clone(), "Comment for file1", cx);
        add_test_comment(editor, key2.clone(), "Comment for file2", cx);

        let snapshot = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(editor.total_review_comment_count(), 2);
        assert_eq!(editor.hunk_comment_count(&key1, &snapshot), 1);
        assert_eq!(editor.hunk_comment_count(&key2, &snapshot), 1);

        assert_eq!(
            editor.comments_for_hunk(&key1, &snapshot)[0].comment,
            "Comment for file1"
        );
        assert_eq!(
            editor.comments_for_hunk(&key2, &snapshot)[0].comment,
            "Comment for file2"
        );
    });
}

#[gpui::test]
fn test_review_comment_remove(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor: &mut Editor, _window, cx| {
        let key = test_hunk_key("");

        let id = add_test_comment(editor, key, "To be removed", cx);

        assert_eq!(editor.total_review_comment_count(), 1);

        let removed = editor.remove_review_comment(id, cx);
        assert!(removed);
        assert_eq!(editor.total_review_comment_count(), 0);

        // Try to remove again
        let removed_again = editor.remove_review_comment(id, cx);
        assert!(!removed_again);
    });
}

#[gpui::test]
fn test_review_comment_update(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor: &mut Editor, _window, cx| {
        let key = test_hunk_key("");

        let id = add_test_comment(editor, key.clone(), "Original text", cx);

        let updated = editor.update_review_comment(id, "Updated text".to_string(), cx);
        assert!(updated);

        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let comments = editor.comments_for_hunk(&key, &snapshot);
        assert_eq!(comments[0].comment, "Updated text");
        assert!(!comments[0].is_editing); // Should clear editing flag
    });
}

#[gpui::test]
fn test_review_comment_take_all(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor: &mut Editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let anchor1 = snapshot.anchor_before(Point::new(0, 0));
        let anchor2 = snapshot.anchor_before(Point::new(0, 0));
        let key1 = test_hunk_key_with_anchor("file1.rs", anchor1);
        let key2 = test_hunk_key_with_anchor("file2.rs", anchor2);

        let id1 = add_test_comment(editor, key1.clone(), "Comment 1", cx);
        let id2 = add_test_comment(editor, key1.clone(), "Comment 2", cx);
        let id3 = add_test_comment(editor, key2.clone(), "Comment 3", cx);

        // IDs should be sequential starting from 0
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);

        assert_eq!(editor.total_review_comment_count(), 3);

        let taken = editor.take_all_review_comments(cx);

        // Should have 2 entries (one per hunk)
        assert_eq!(taken.len(), 2);

        // Total comments should be 3
        let total: usize = taken
            .iter()
            .map(|(_, comments): &(DiffHunkKey, Vec<StoredReviewComment>)| comments.len())
            .sum();
        assert_eq!(total, 3);

        // Storage should be empty
        assert_eq!(editor.total_review_comment_count(), 0);

        // After taking all comments, ID counter should reset
        // New comments should get IDs starting from 0 again
        let new_id1 = add_test_comment(editor, key1, "New Comment 1", cx);
        let new_id2 = add_test_comment(editor, key2, "New Comment 2", cx);

        assert_eq!(new_id1, 0, "ID counter should reset after take_all");
        assert_eq!(new_id2, 1, "IDs should be sequential after reset");
    });
}

#[gpui::test]
fn test_diff_review_overlay_show_and_dismiss(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify overlay is shown
    editor
        .update(cx, |editor, _window, cx| {
            assert!(!editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), Some((0, 0)));
            assert!(editor.diff_review_prompt_editor().is_some());
        })
        .unwrap();

    // Dismiss overlay
    editor
        .update(cx, |editor, _window, cx| {
            editor.dismiss_all_diff_review_overlays(cx);
        })
        .unwrap();

    // Verify overlay is dismissed
    editor
        .update(cx, |editor, _window, cx| {
            assert!(editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), None);
            assert!(editor.diff_review_prompt_editor().is_none());
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_overlay_dismiss_via_cancel(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify overlay is shown
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(!editor.diff_review_overlays.is_empty());
        })
        .unwrap();

    // Dismiss via dismiss_menus_and_popups (which is called by cancel action)
    editor
        .update(cx, |editor, window, cx| {
            editor.dismiss_menus_and_popups(true, window, cx);
        })
        .unwrap();

    // Verify overlay is dismissed
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_overlays.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_empty_comment_not_submitted(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Try to submit without typing anything (empty comment)
    editor
        .update(cx, |editor, window, cx| {
            editor.submit_diff_review_comment(window, cx);
        })
        .unwrap();

    // Verify no comment was added
    editor
        .update(cx, |editor, _window, _cx| {
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();

    // Try to submit with whitespace-only comment
    editor
        .update(cx, |editor, window, cx| {
            if let Some(prompt_editor) = editor.diff_review_prompt_editor().cloned() {
                prompt_editor.update(cx, |pe, cx| {
                    pe.insert("   \n\t  ", window, cx);
                });
            }
            editor.submit_diff_review_comment(window, cx);
        })
        .unwrap();

    // Verify still no comment was added
    editor
        .update(cx, |editor, _window, _cx| {
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_inline_edit_flow(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Add a comment directly
    let comment_id = editor
        .update(cx, |editor, _window, cx| {
            let key = test_hunk_key("");
            add_test_comment(editor, key, "Original comment", cx)
        })
        .unwrap();

    // Set comment to editing mode
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_comment_editing(comment_id, true, cx);
        })
        .unwrap();

    // Verify editing flag is set
    editor
        .update(cx, |editor, _window, cx| {
            let key = test_hunk_key("");
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let comments = editor.comments_for_hunk(&key, &snapshot);
            assert_eq!(comments.len(), 1);
            assert!(comments[0].is_editing);
        })
        .unwrap();

    // Update the comment
    editor
        .update(cx, |editor, _window, cx| {
            let updated =
                editor.update_review_comment(comment_id, "Updated comment".to_string(), cx);
            assert!(updated);
        })
        .unwrap();

    // Verify comment was updated and editing flag is cleared
    editor
        .update(cx, |editor, _window, cx| {
            let key = test_hunk_key("");
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let comments = editor.comments_for_hunk(&key, &snapshot);
            assert_eq!(comments[0].comment, "Updated comment");
            assert!(!comments[0].is_editing);
        })
        .unwrap();
}

#[gpui::test]
fn test_orphaned_comments_are_cleaned_up(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Create an editor with some text
    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Add a comment with an anchor on line 2
    editor
        .update(cx, |editor, _window, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let anchor = snapshot.anchor_after(Point::new(1, 0)); // Line 2
            let key = DiffHunkKey {
                file_path: Arc::from(util::rel_path::RelPath::empty()),
                hunk_start_anchor: anchor,
            };
            editor.add_review_comment(key, "Comment on line 2".to_string(), anchor..anchor, cx);
            assert_eq!(editor.total_review_comment_count(), 1);
        })
        .unwrap();

    // Delete all content (this should orphan the comment's anchor)
    editor
        .update(cx, |editor, window, cx| {
            editor.select_all(&SelectAll, window, cx);
            editor.insert("completely new content", window, cx);
        })
        .unwrap();

    // Trigger cleanup
    editor
        .update(cx, |editor, _window, cx| {
            editor.cleanup_orphaned_review_comments(cx);
            // Comment should be removed because its anchor is invalid
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn test_orphaned_comments_cleanup_called_on_buffer_edit(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Create an editor with some text
    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Add a comment with an anchor on line 2
    editor
        .update(cx, |editor, _window, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let anchor = snapshot.anchor_after(Point::new(1, 0)); // Line 2
            let key = DiffHunkKey {
                file_path: Arc::from(util::rel_path::RelPath::empty()),
                hunk_start_anchor: anchor,
            };
            editor.add_review_comment(key, "Comment on line 2".to_string(), anchor..anchor, cx);
            assert_eq!(editor.total_review_comment_count(), 1);
        })
        .unwrap();

    // Edit the buffer - this should trigger cleanup via on_buffer_event
    // Delete all content which orphans the anchor
    editor
        .update(cx, |editor, window, cx| {
            editor.select_all(&SelectAll, window, cx);
            editor.insert("completely new content", window, cx);
            // The cleanup is called automatically in on_buffer_event when Edited fires
        })
        .unwrap();

    // Verify cleanup happened automatically (not manually triggered)
    editor
        .update(cx, |editor, _window, _cx| {
            // Comment should be removed because its anchor became invalid
            // and cleanup was called automatically on buffer edit
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn test_comments_stored_for_multiple_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that comments can be stored for multiple different hunks
    // and that hunk_comment_count correctly identifies comments per hunk.
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);

        // Create two different hunk keys (simulating two different files)
        let anchor = snapshot.anchor_before(Point::new(0, 0));
        let key1 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file1.rs").unwrap()),
            hunk_start_anchor: anchor,
        };
        let key2 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file2.rs").unwrap()),
            hunk_start_anchor: anchor,
        };

        // Add comments to first hunk
        editor.add_review_comment(
            key1.clone(),
            "Comment 1 for file1".to_string(),
            anchor..anchor,
            cx,
        );
        editor.add_review_comment(
            key1.clone(),
            "Comment 2 for file1".to_string(),
            anchor..anchor,
            cx,
        );

        // Add comment to second hunk
        editor.add_review_comment(
            key2.clone(),
            "Comment for file2".to_string(),
            anchor..anchor,
            cx,
        );

        // Verify total count
        assert_eq!(editor.total_review_comment_count(), 3);

        // Verify per-hunk counts
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(
            editor.hunk_comment_count(&key1, &snapshot),
            2,
            "file1 should have 2 comments"
        );
        assert_eq!(
            editor.hunk_comment_count(&key2, &snapshot),
            1,
            "file2 should have 1 comment"
        );

        // Verify comments_for_hunk returns correct comments
        let file1_comments = editor.comments_for_hunk(&key1, &snapshot);
        assert_eq!(file1_comments.len(), 2);
        assert_eq!(file1_comments[0].comment, "Comment 1 for file1");
        assert_eq!(file1_comments[1].comment, "Comment 2 for file1");

        let file2_comments = editor.comments_for_hunk(&key2, &snapshot);
        assert_eq!(file2_comments.len(), 1);
        assert_eq!(file2_comments[0].comment, "Comment for file2");
    });
}

#[gpui::test]
fn test_same_hunk_detected_by_matching_keys(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that hunk_keys_match correctly identifies when two
    // DiffHunkKeys refer to the same hunk (same file path and anchor point).
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let anchor = snapshot.anchor_before(Point::new(0, 0));

        // Create two keys with the same file path and anchor
        let key1 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file.rs").unwrap()),
            hunk_start_anchor: anchor,
        };
        let key2 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file.rs").unwrap()),
            hunk_start_anchor: anchor,
        };

        // Add comment to first key
        editor.add_review_comment(key1, "Test comment".to_string(), anchor..anchor, cx);

        // Verify second key (same hunk) finds the comment
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(
            editor.hunk_comment_count(&key2, &snapshot),
            1,
            "Same hunk should find the comment"
        );

        // Create a key with different file path
        let different_file_key = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("other.rs").unwrap()),
            hunk_start_anchor: anchor,
        };

        // Different file should not find the comment
        assert_eq!(
            editor.hunk_comment_count(&different_file_key, &snapshot),
            0,
            "Different file should not find the comment"
        );
    });
}

#[gpui::test]
fn test_overlay_comments_expanded_state(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that set_diff_review_comments_expanded correctly
    // updates the expanded state of overlays.
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify initially expanded (default)
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(
                editor.diff_review_overlays[0].comments_expanded,
                "Should be expanded by default"
            );
        })
        .unwrap();

    // Set to collapsed using the public method
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_diff_review_comments_expanded(false, cx);
        })
        .unwrap();

    // Verify collapsed
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(
                !editor.diff_review_overlays[0].comments_expanded,
                "Should be collapsed after setting to false"
            );
        })
        .unwrap();

    // Set back to expanded
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_diff_review_comments_expanded(true, cx);
        })
        .unwrap();

    // Verify expanded again
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(
                editor.diff_review_overlays[0].comments_expanded,
                "Should be expanded after setting to true"
            );
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_multiline_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Create an editor with multiple lines of text
    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\nline 4\nline 5\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Test showing overlay with a multi-line selection (lines 1-3, which are rows 0-2)
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(2), window, cx);
        })
        .unwrap();

    // Verify line range
    editor
        .update(cx, |editor, _window, cx| {
            assert!(!editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), Some((0, 2)));
        })
        .unwrap();

    // Dismiss and test with reversed range (end < start)
    editor
        .update(cx, |editor, _window, cx| {
            editor.dismiss_all_diff_review_overlays(cx);
        })
        .unwrap();

    // Show overlay with reversed range - should normalize it
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(3)..DisplayRow(1), window, cx);
        })
        .unwrap();

    // Verify range is normalized (start <= end)
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(editor.diff_review_line_range(cx), Some((1, 3)));
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_drag_state(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Initially no drag state
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_drag_state.is_none());
        })
        .unwrap();

    // Start drag at row 1
    editor
        .update(cx, |editor, window, cx| {
            editor.start_diff_review_drag(DisplayRow(1), window, cx);
        })
        .unwrap();

    // Verify drag state is set
    editor
        .update(cx, |editor, window, cx| {
            assert!(editor.diff_review_drag_state.is_some());
            let snapshot = editor.snapshot(window, cx);
            let range = editor
                .diff_review_drag_state
                .as_ref()
                .unwrap()
                .row_range(&snapshot.display_snapshot);
            assert_eq!(*range.start(), DisplayRow(1));
            assert_eq!(*range.end(), DisplayRow(1));
        })
        .unwrap();

    // Update drag to row 3
    editor
        .update(cx, |editor, window, cx| {
            editor.update_diff_review_drag(DisplayRow(3), window, cx);
        })
        .unwrap();

    // Verify drag state is updated
    editor
        .update(cx, |editor, window, cx| {
            assert!(editor.diff_review_drag_state.is_some());
            let snapshot = editor.snapshot(window, cx);
            let range = editor
                .diff_review_drag_state
                .as_ref()
                .unwrap()
                .row_range(&snapshot.display_snapshot);
            assert_eq!(*range.start(), DisplayRow(1));
            assert_eq!(*range.end(), DisplayRow(3));
        })
        .unwrap();

    // End drag - should show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.end_diff_review_drag(window, cx);
        })
        .unwrap();

    // Verify drag state is cleared and overlay is shown
    editor
        .update(cx, |editor, _window, cx| {
            assert!(editor.diff_review_drag_state.is_none());
            assert!(!editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), Some((1, 3)));
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_drag_cancel(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Start drag
    editor
        .update(cx, |editor, window, cx| {
            editor.start_diff_review_drag(DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify drag state is set
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_drag_state.is_some());
        })
        .unwrap();

    // Cancel drag
    editor
        .update(cx, |editor, _window, cx| {
            editor.cancel_diff_review_drag(cx);
        })
        .unwrap();

    // Verify drag state is cleared and no overlay was created
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_drag_state.is_none());
            assert!(editor.diff_review_overlays.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn test_calculate_overlay_height(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that calculate_overlay_height returns correct heights
    // based on comment count and expanded state.
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let anchor = snapshot.anchor_before(Point::new(0, 0));
        let key = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::empty()),
            hunk_start_anchor: anchor,
        };

        // No comments: base height of 2
        let height_no_comments = editor.calculate_overlay_height(&key, true, &snapshot);
        assert_eq!(
            height_no_comments, 2,
            "Base height should be 2 with no comments"
        );

        // Add one comment
        editor.add_review_comment(key.clone(), "Comment 1".to_string(), anchor..anchor, cx);

        let snapshot = editor.buffer().read(cx).snapshot(cx);

        // With comments expanded: base (2) + header (1) + 2 per comment
        let height_expanded = editor.calculate_overlay_height(&key, true, &snapshot);
        assert_eq!(
            height_expanded,
            2 + 1 + 2, // base + header + 1 comment * 2
            "Height with 1 comment expanded"
        );

        // With comments collapsed: base (2) + header (1)
        let height_collapsed = editor.calculate_overlay_height(&key, false, &snapshot);
        assert_eq!(
            height_collapsed,
            2 + 1, // base + header only
            "Height with comments collapsed"
        );

        // Add more comments
        editor.add_review_comment(key.clone(), "Comment 2".to_string(), anchor..anchor, cx);
        editor.add_review_comment(key.clone(), "Comment 3".to_string(), anchor..anchor, cx);

        let snapshot = editor.buffer().read(cx).snapshot(cx);

        // With 3 comments expanded
        let height_3_expanded = editor.calculate_overlay_height(&key, true, &snapshot);
        assert_eq!(
            height_3_expanded,
            2 + 1 + (3 * 2), // base + header + 3 comments * 2
            "Height with 3 comments expanded"
        );

        // Collapsed height stays the same regardless of comment count
        let height_3_collapsed = editor.calculate_overlay_height(&key, false, &snapshot);
        assert_eq!(
            height_3_collapsed,
            2 + 1, // base + header only
            "Height with 3 comments collapsed should be same as 1 comment collapsed"
        );
    });
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

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
#[path = "editor_tests/basic_navigation.rs"]
mod basic_navigation;
#[path = "editor_tests/block_operations.rs"]
mod block_operations;
#[path = "editor_tests/breakpoints_basic.rs"]
mod breakpoints_basic;
#[path = "editor_tests/breakpoints_state.rs"]
mod breakpoints_state;
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
#[path = "editor_tests/editor_ui_interactions.rs"]
mod editor_ui_interactions;
#[path = "editor_tests/events_input.rs"]
mod events_input;
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
#[path = "editor_tests/join_lines_comments.rs"]
mod join_lines_comments;
#[path = "editor_tests/line_operations.rs"]
mod line_operations;
#[path = "editor_tests/lsp_formatting_restart.rs"]
mod lsp_formatting_restart;
#[path = "editor_tests/manual_formatting.rs"]
mod manual_formatting;
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
#[path = "editor_tests/range_formatting.rs"]
mod range_formatting;
#[path = "editor_tests/reference_task_context.rs"]
mod reference_task_context;
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
#[path = "editor_tests/selection_split.rs"]
mod selection_split;
#[path = "editor_tests/signature_help_display.rs"]
mod signature_help_display;
#[path = "editor_tests/signature_help_input.rs"]
mod signature_help_input;
#[path = "editor_tests/snippet_insertion.rs"]
mod snippet_insertion;
#[path = "editor_tests/snippet_menu_refresh.rs"]
mod snippet_menu_refresh;
#[path = "editor_tests/snippet_remaining.rs"]
mod snippet_remaining;
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

struct BookmarkTestContext {
    project: Entity<Project>,
    editor: Entity<Editor>,
    cx: VisualTestContext,
}

impl BookmarkTestContext {
    async fn new(sample_text: &str, cx: &mut TestAppContext) -> BookmarkTestContext {
        init_test(cx, |_| {});

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/a"),
            json!({
                "main.rs": sample_text,
            }),
        )
        .await;
        let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let mut visual_cx = VisualTestContext::from_window(*window, cx);
        let worktree_id = workspace.update_in(&mut visual_cx, |workspace, _window, cx| {
            workspace.project().update(cx, |project, cx| {
                project.worktrees(cx).next().unwrap().read(cx).id()
            })
        });

        let buffer = project
            .update(&mut visual_cx, |project, cx| {
                project.open_buffer((worktree_id, rel_path("main.rs")), cx)
            })
            .await
            .unwrap();

        let (editor, editor_cx) = cx.add_window_view(|window, cx| {
            Editor::new(
                EditorMode::full(),
                MultiBuffer::build_from_buffer(buffer, cx),
                Some(project.clone()),
                window,
                cx,
            )
        });
        let cx = editor_cx.clone();

        BookmarkTestContext {
            project,
            editor,
            cx,
        }
    }

    fn abs_path(&self) -> Arc<Path> {
        let project_path = self.editor.read_with(&self.cx, |editor, cx| {
            editor.active_project_path(cx).unwrap()
        });
        self.project.read_with(&self.cx, |project, cx| {
            project
                .absolute_path(&project_path, cx)
                .map(Arc::from)
                .unwrap()
        })
    }

    fn all_bookmarks(&self) -> BTreeMap<Arc<Path>, Vec<SerializedBookmark>> {
        self.project.read_with(&self.cx, |project, cx| {
            project
                .bookmark_store()
                .read(cx)
                .all_serialized_bookmarks(cx)
        })
    }

    fn assert_bookmarked_file_count(&self, expected_count: usize) {
        assert_eq!(expected_count, self.all_bookmarks().len());
    }

    fn assert_bookmark_rows(&self, expected_rows: Vec<u32>) {
        let abs_path = self.abs_path();
        let bookmarks = self.all_bookmarks();
        if expected_rows.is_empty() {
            assert!(
                !bookmarks.contains_key(&abs_path),
                "Expected no bookmarks for {}",
                abs_path.display()
            );
        } else {
            let mut rows: Vec<u32> = bookmarks
                .get(&abs_path)
                .unwrap()
                .iter()
                .map(|b| b.row)
                .collect();
            rows.sort();
            assert_eq!(expected_rows, rows);
        }
    }

    fn assert_bookmark_labels(&self, expected_labels: Vec<(u32, &str)>) {
        let abs_path = self.abs_path();
        let bookmarks = self.all_bookmarks();
        let mut labels: Vec<(u32, &str)> = bookmarks
            .get(&abs_path)
            .unwrap()
            .iter()
            .map(|bookmark| (bookmark.row, bookmark.label.as_str()))
            .collect();
        labels.sort_by_key(|(row, _)| *row);
        assert_eq!(expected_labels, labels);
    }

    fn confirm_action_available(&mut self) -> bool {
        self.cx
            .update(|window, cx| window.is_action_available(&menu::Confirm, cx))
    }

    fn select_rows(&mut self, rows: &[u32]) {
        assert!(!rows.is_empty(), "expected at least one row to select");

        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_ranges(
                        rows.iter()
                            .copied()
                            .map(|row| Point::new(row, 0)..Point::new(row, 0)),
                    )
                });
            });
    }

    fn prompt_blocks(&mut self) -> Vec<(DisplayRow, u32)> {
        self.editor.update(&mut self.cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            let max_row = snapshot.max_point().row().next_row();

            snapshot
                .blocks_in_range(DisplayRow(0)..max_row)
                .filter_map(|(row, block)| match block {
                    crate::display_map::Block::Custom(_) => Some((row, block.height())),
                    _ => None,
                })
                .collect()
        })
    }

    fn assert_prompt_block_count(&mut self, expected_count: usize) {
        assert_eq!(expected_count, self.prompt_blocks().len());
    }

    fn draw_window(&mut self) {
        self.cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
    }

    fn focus_bookmark_prompt_block(&mut self, block_index: usize) {
        self.draw_window();

        let prompt_blocks = self.prompt_blocks();
        let (block_row, block_height) = *prompt_blocks
            .get(block_index)
            .expect("expected bookmark prompt block");

        let click_position =
            self.editor
                .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                    let snapshot = editor.snapshot(window, cx);
                    let block_top = DisplayPoint::new(block_row, 0);
                    let relative_block_top = editor
                        .display_to_pixel_point(block_top, &snapshot, window, cx)
                        .expect("expected prompt block to be visible");
                    let line_height = editor
                        .style(cx)
                        .text
                        .line_height_in_pixels(window.rem_size());
                    let editor_origin = editor
                        .last_position_map
                        .as_ref()
                        .expect("expected editor position map")
                        .text_hitbox
                        .bounds
                        .origin;
                    let editor_center_x = editor
                        .last_bounds
                        .expect("expected editor bounds")
                        .center()
                        .x;

                    gpui::Point {
                        x: editor_center_x,
                        y: editor_origin.y
                            + relative_block_top.y
                            + line_height * (block_height as f32 / 2.),
                    }
                });

        self.cx
            .simulate_click(click_position, gpui::Modifiers::none());
        self.cx.run_until_parked();

        assert!(
            self.confirm_action_available(),
            "expected bookmark prompt block to be focused"
        );
    }

    fn confirm_bookmark_prompt_at_block_index(&mut self, block_index: usize, label: &str) {
        // Confirming a PromptEditor returns focus to the parent editor, so each remaining
        // prompt block must be focused explicitly before typing into it.
        self.focus_bookmark_prompt_block(block_index);
        self.confirm_bookmark_prompt(label);
    }

    fn cursor_row(&mut self) -> u32 {
        self.cursor_point().row
    }

    fn cursor_point(&mut self) -> Point {
        self.editor.update(&mut self.cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            editor.selections.newest::<Point>(&snapshot).head()
        })
    }

    fn move_to_row(&mut self, row: u32) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.move_to_beginning(&MoveToBeginning, window, cx);
                for _ in 0..row {
                    editor.move_down(&MoveDown, window, cx);
                }
            });
    }

    fn toggle_bookmark(&mut self) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.toggle_bookmark(&actions::ToggleBookmark, window, cx);
            });
    }

    fn confirm_bookmark_prompt(&mut self, label: &str) {
        if !label.is_empty() {
            self.cx.simulate_input(label);
        }
        self.cx.dispatch_action(menu::Confirm);
        self.cx.run_until_parked();
    }

    fn add_bookmark_with_label(&mut self, label: &str) {
        self.toggle_bookmark();
        self.confirm_bookmark_prompt(label);
    }

    fn toggle_bookmarks_at_rows(&mut self, rows: &[u32]) {
        for &row in rows {
            self.move_to_row(row);
            self.add_bookmark_with_label("");
        }
    }

    fn go_to_next_bookmark(&mut self) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.go_to_next_bookmark(&actions::GoToNextBookmark, window, cx);
            });
    }

    fn go_to_previous_bookmark(&mut self) {
        self.editor
            .update_in(&mut self.cx, |editor: &mut Editor, window, cx| {
                editor.go_to_previous_bookmark(&actions::GoToPreviousBookmark, window, cx);
            });
    }
}

#[gpui::test]
async fn test_bookmark_toggling(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.add_bookmark_with_label("");
    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_end(&MoveToEnd, window, cx);
        });
    ctx.add_bookmark_with_label("");

    ctx.assert_bookmarked_file_count(1);
    ctx.assert_bookmark_rows(vec![0, 3]);

    ctx.move_to_row(0);
    ctx.toggle_bookmark();

    ctx.assert_bookmarked_file_count(1);
    ctx.assert_bookmark_rows(vec![3]);

    ctx.move_to_row(3);
    ctx.toggle_bookmark();

    ctx.assert_bookmarked_file_count(0);
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_bookmark_toggling_with_multiple_selections(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.select_rows(&[0, 1, 2]);
    ctx.toggle_bookmark();

    ctx.assert_prompt_block_count(3);
    ctx.assert_bookmarked_file_count(0);

    ctx.confirm_bookmark_prompt_at_block_index(0, "first label");
    ctx.assert_prompt_block_count(2);
    ctx.confirm_bookmark_prompt_at_block_index(0, "second label");
    ctx.assert_prompt_block_count(1);
    ctx.confirm_bookmark_prompt_at_block_index(0, "third label");
    ctx.assert_prompt_block_count(0);

    ctx.assert_bookmarked_file_count(1);
    ctx.assert_bookmark_labels(vec![
        (0, "first label"),
        (1, "second label"),
        (2, "third label"),
    ]);

    ctx.select_rows(&[0, 1, 2, 3]);
    ctx.toggle_bookmark();

    ctx.assert_prompt_block_count(1);
    ctx.assert_bookmark_labels(vec![
        (0, "first label"),
        (1, "second label"),
        (2, "third label"),
    ]);

    ctx.confirm_bookmark_prompt_at_block_index(0, "fourth label");

    ctx.assert_prompt_block_count(0);
    ctx.assert_bookmark_labels(vec![
        (0, "first label"),
        (1, "second label"),
        (2, "third label"),
        (3, "fourth label"),
    ]);

    ctx.select_rows(&[0, 1, 2, 3]);
    ctx.toggle_bookmark();

    ctx.assert_prompt_block_count(0);
    ctx.assert_bookmarked_file_count(0);
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_bookmark_toggle_deduplicates_by_row(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
        });
    ctx.add_bookmark_with_label("");

    ctx.assert_bookmark_rows(vec![0]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_end_of_line(
                &MoveToEndOfLine {
                    stop_at_soft_wraps: true,
                },
                window,
                cx,
            );
        });
    ctx.toggle_bookmark();

    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_bookmark_survives_edits(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.move_to_row(2);
    ctx.add_bookmark_with_label("");
    ctx.assert_bookmark_rows(vec![2]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
            editor.newline(&Newline, window, cx);
        });

    ctx.assert_bookmark_rows(vec![3]);

    ctx.move_to_row(3);
    ctx.toggle_bookmark();
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_active_bookmarks(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[1, 3, 5, 8]);

    let active = ctx
        .editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.active_bookmarks(DisplayRow(0)..DisplayRow(10), window, cx)
        });
    assert!(active.contains(&DisplayRow(1)));
    assert!(active.contains(&DisplayRow(3)));
    assert!(active.contains(&DisplayRow(5)));
    assert!(active.contains(&DisplayRow(8)));
    assert!(!active.contains(&DisplayRow(0)));
    assert!(!active.contains(&DisplayRow(2)));
    assert!(!active.contains(&DisplayRow(9)));

    let active = ctx
        .editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.active_bookmarks(DisplayRow(2)..DisplayRow(6), window, cx)
        });
    assert!(active.contains(&DisplayRow(3)));
    assert!(active.contains(&DisplayRow(5)));
    assert!(!active.contains(&DisplayRow(1)));
    assert!(!active.contains(&DisplayRow(8)));
}

#[gpui::test]
async fn test_bookmark_not_available_in_single_line_editor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (editor, _cx) = cx.add_window_view(|window, cx| Editor::single_line(window, cx));

    editor.update(cx, |editor, _cx| {
        assert!(
            editor.bookmark_store.is_none(),
            "Single-line editors should not have a bookmark store"
        );
    });
}

#[gpui::test]
async fn test_edit_bookmark_does_not_open_prompt_without_existing_bookmark(
    cx: &mut TestAppContext,
) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    assert!(!ctx.confirm_action_available());

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.edit_bookmark(&actions::EditBookmark, window, cx);
        });

    assert!(!ctx.confirm_action_available());
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_edit_bookmark_updates_label_after_confirmation(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.add_bookmark_with_label("old label");
    ctx.assert_bookmark_labels(vec![(0, "old label")]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.edit_bookmark(&actions::EditBookmark, window, cx);
        });

    assert!(ctx.confirm_action_available());
    ctx.cx.dispatch_action(SelectAll);
    ctx.cx.simulate_input("new label");
    ctx.cx.dispatch_action(menu::Confirm);

    ctx.assert_bookmark_labels(vec![(0, "new label")]);
}

#[gpui::test]
async fn test_bookmark_navigation_lands_at_column_zero(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
            editor.move_down(&MoveDown, window, cx);
            editor.move_to_end_of_line(
                &MoveToEndOfLine {
                    stop_at_soft_wraps: true,
                },
                window,
                cx,
            );
        });

    let column_before_toggle = ctx.cursor_point().column;
    assert_eq!(
        column_before_toggle, 11,
        "Cursor should be at the 11th column before toggling bookmark, got column {column_before_toggle}"
    );

    ctx.add_bookmark_with_label("");

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
        });

    ctx.go_to_next_bookmark();

    let cursor = ctx.cursor_point();
    assert_eq!(cursor.row, 1, "Should navigate to the bookmarked row");
    assert_eq!(
        cursor.column, 0,
        "Bookmark navigation should always land at column 0"
    );
}

#[gpui::test]
async fn test_bookmark_set_from_nonzero_column_toggles_off_from_column_zero(
    cx: &mut TestAppContext,
) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
            editor.move_down(&MoveDown, window, cx);
            editor.move_to_end_of_line(
                &MoveToEndOfLine {
                    stop_at_soft_wraps: true,
                },
                window,
                cx,
            );
        });
    ctx.add_bookmark_with_label("");

    ctx.assert_bookmark_rows(vec![1]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning_of_line(
                &MoveToBeginningOfLine {
                    stop_at_soft_wraps: true,
                    stop_at_indent: false,
                },
                window,
                cx,
            );
        });
    ctx.toggle_bookmark();

    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_go_to_next_bookmark(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[2, 5, 8]);

    ctx.move_to_row(0);

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        2,
        "First next-bookmark should go to row 2"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        5,
        "Second next-bookmark should go to row 5"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "Third next-bookmark should go to row 8"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        2,
        "Next-bookmark should wrap around to row 2"
    );
}

#[gpui::test]
async fn test_go_to_previous_bookmark(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[2, 5, 8]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_end(&MoveToEnd, window, cx);
        });

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "First prev-bookmark should go to row 8"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        5,
        "Second prev-bookmark should go to row 5"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        2,
        "Third prev-bookmark should go to row 2"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "Prev-bookmark should wrap around to row 8"
    );
}

#[gpui::test]
async fn test_go_to_bookmark_when_cursor_on_bookmarked_line(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[3, 7]);

    ctx.move_to_row(3);

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        7,
        "Next from bookmarked row 3 should go to row 7"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        3,
        "Previous from bookmarked row 7 should go to row 3"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 7, "Next from row 3 should go to row 7");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 3, "Next from row 7 should wrap to row 3");
}

#[gpui::test]
async fn test_go_to_bookmark_with_out_of_order_bookmarks(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[8, 1, 5]);

    ctx.move_to_row(0);

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 1, "First next should go to row 1");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 5, "Second next should go to row 5");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 8, "Third next should go to row 8");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 1, "Fourth next should wrap to row 1");

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "Prev from row 1 should wrap around to row 8"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(ctx.cursor_row(), 5, "Prev from row 8 should go to row 5");

    ctx.go_to_previous_bookmark();
    assert_eq!(ctx.cursor_row(), 1, "Prev from row 5 should go to row 1");
}

#[gpui::test]
async fn test_rename_with_duplicate_edits(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let capabilities = lsp::ServerCapabilities {
        rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        ..Default::default()
    };
    let mut cx = EditorLspTestContext::new_rust(capabilities, cx).await;

    cx.set_state(indoc! {"
        struct Fˇoo {}
    "});

    cx.update_editor(|editor, _, cx| {
        let highlight_range = Point::new(0, 7)..Point::new(0, 10);
        let highlight_range = highlight_range.to_anchors(&editor.buffer().read(cx).snapshot(cx));
        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &[highlight_range],
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
    });

    let mut prepare_rename_handler = cx
        .set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(
            move |_, _, _| async move {
                Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 7,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 10,
                    },
                })))
            },
        );
    let prepare_rename_task = cx
        .update_editor(|e, window, cx| e.rename(&Rename, window, cx))
        .expect("Prepare rename was not started");
    prepare_rename_handler.next().await.unwrap();
    prepare_rename_task.await.expect("Prepare rename failed");

    let mut rename_handler =
        cx.set_request_handler::<lsp::request::Rename, _, _>(move |url, _, _| async move {
            let edit = lsp::TextEdit {
                range: lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 7,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 10,
                    },
                },
                new_text: "FooRenamed".to_string(),
            };
            Ok(Some(lsp::WorkspaceEdit::new(
                // Specify the same edit twice
                std::collections::HashMap::from_iter(Some((url, vec![edit.clone(), edit]))),
            )))
        });
    let rename_task = cx
        .update_editor(|e, window, cx| e.confirm_rename(&ConfirmRename, window, cx))
        .expect("Confirm rename was not started");
    rename_handler.next().await.unwrap();
    rename_task.await.expect("Confirm rename failed");
    cx.run_until_parked();

    // Despite two edits, only one is actually applied as those are identical
    cx.assert_editor_state(indoc! {"
        struct FooRenamedˇ {}
    "});
}

#[gpui::test]
async fn test_rename_with_out_of_order_document_highlights(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let capabilities = lsp::ServerCapabilities {
        rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        ..Default::default()
    };
    let mut cx = EditorLspTestContext::new_rust(capabilities, cx).await;

    cx.set_state(indoc! {"
        struct Foo {}
        fn main() {
            let first = Foo {};
            let second = Fˇoo {};
        }
    "});

    cx.update_editor(|editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let read_highlight = (Point::new(2, 16)..Point::new(2, 19)).to_anchors(&snapshot);
        let write_highlight = (Point::new(3, 17)..Point::new(3, 20)).to_anchors(&snapshot);
        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &[read_highlight],
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
        editor.highlight_background(
            HighlightKey::DocumentHighlightWrite,
            &[write_highlight],
            |_, theme| theme.colors().editor_document_highlight_write_background,
            cx,
        );
    });

    let mut prepare_rename_handler = cx
        .set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(
            move |_, _, _| async move {
                Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range {
                    start: lsp::Position {
                        line: 3,
                        character: 17,
                    },
                    end: lsp::Position {
                        line: 3,
                        character: 20,
                    },
                })))
            },
        );
    let prepare_rename_task = cx
        .update_editor(|e, window, cx| e.rename(&Rename, window, cx))
        .expect("Prepare rename was not started");
    prepare_rename_handler.next().await.unwrap();
    prepare_rename_task.await.expect("Prepare rename failed");

    cx.update_editor(|editor, window, cx| {
        editor
            .snapshot(window, cx)
            .layout_row(DisplayRow(2), &editor.text_layout_details(window, cx));
    });
}

#[gpui::test]
async fn test_rename_without_prepare(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    // These capabilities indicate that the server does not support prepare rename.
    let capabilities = lsp::ServerCapabilities {
        rename_provider: Some(lsp::OneOf::Left(true)),
        ..Default::default()
    };
    let mut cx = EditorLspTestContext::new_rust(capabilities, cx).await;

    cx.set_state(indoc! {"
        struct Fˇoo {}
    "});

    cx.update_editor(|editor, _window, cx| {
        let highlight_range = Point::new(0, 7)..Point::new(0, 10);
        let highlight_range = highlight_range.to_anchors(&editor.buffer().read(cx).snapshot(cx));
        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &[highlight_range],
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
    });

    cx.update_editor(|e, window, cx| e.rename(&Rename, window, cx))
        .expect("Prepare rename was not started")
        .await
        .expect("Prepare rename failed");

    let mut rename_handler =
        cx.set_request_handler::<lsp::request::Rename, _, _>(move |url, _, _| async move {
            let edit = lsp::TextEdit {
                range: lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 7,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 10,
                    },
                },
                new_text: "FooRenamed".to_string(),
            };
            Ok(Some(lsp::WorkspaceEdit::new(
                std::collections::HashMap::from_iter(Some((url, vec![edit]))),
            )))
        });
    let rename_task = cx
        .update_editor(|e, window, cx| e.confirm_rename(&ConfirmRename, window, cx))
        .expect("Confirm rename was not started");
    rename_handler.next().await.unwrap();
    rename_task.await.expect("Confirm rename failed");
    cx.run_until_parked();

    // Correct range is renamed, as `surrounding_word` is used to find it.
    cx.assert_editor_state(indoc! {"
        struct FooRenamedˇ {}
    "});
}

#[gpui::test]
async fn test_tree_sitter_brackets_newline_insertion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_html::LANGUAGE.into()),
        )
        .with_brackets_query(
            r#"
            ("<" @open "/>" @close)
            ("</" @open ">" @close)
            ("<" @open ">" @close)
            ("\"" @open "\"" @close)
            ((element (start_tag) @open (end_tag) @close) (#set! newline.only))
        "#,
        )
        .unwrap(),
    );
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    cx.set_state(indoc! {"
        <span>ˇ</span>
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        <span>
        ˇ
        </span>
    "});

    cx.set_state(indoc! {"
        <span><span></span>ˇ</span>
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        <span><span></span>
        ˇ</span>
    "});

    cx.set_state(indoc! {"
        <span>ˇ
        </span>
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        <span>
        ˇ
        </span>
    "});
}

#[gpui::test(iterations = 10)]
async fn test_apply_code_lens_actions_with_commands(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(settings::CodeLens::Menu);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "a",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )));
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                code_lens_provider: Some(lsp::CodeLensOptions {
                    resolve_provider: Some(true),
                }),
                execute_command_provider: Some(lsp::ExecuteCommandOptions {
                    commands: vec!["_the/command".to_string()],
                    ..lsp::ExecuteCommandOptions::default()
                }),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/dir/a.ts")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    cx.executor().run_until_parked();

    let fake_server = fake_language_servers.next().await.unwrap();

    let buffer = editor.update(cx, |editor, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .expect("have opened a single file by path")
    });

    let buffer_snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
    let anchor = buffer_snapshot.anchor_at(0, text::Bias::Left);
    drop(buffer_snapshot);
    let actions = cx
        .update_window(*window, |_, window, cx| {
            project.code_actions(&buffer, anchor..anchor, window, cx)
        })
        .unwrap();

    fake_server
        .set_request_handler::<lsp::request::CodeLensRequest, _, _>(|_, _| async move {
            Ok(Some(vec![
                lsp::CodeLens {
                    range: lsp::Range::default(),
                    command: Some(lsp::Command {
                        title: "Code lens command".to_owned(),
                        command: "_the/command".to_owned(),
                        arguments: None,
                    }),
                    data: None,
                },
                lsp::CodeLens {
                    range: lsp::Range {
                        start: lsp::Position {
                            line: 1,
                            character: 1,
                        },
                        end: lsp::Position {
                            line: 1,
                            character: 1,
                        },
                    },
                    command: Some(lsp::Command {
                        title: "Command not in range".to_owned(),
                        command: "_the/command".to_owned(),
                        arguments: None,
                    }),
                    data: None,
                },
            ]))
        })
        .next()
        .await;

    let actions = actions.await.unwrap();
    assert_eq!(
        actions.len(),
        1,
        "Should have only one valid action for the 0..0 range, got: {actions:#?}"
    );
    let action = actions[0].clone();
    let apply = project.update(cx, |project, cx| {
        project.apply_code_action(buffer.clone(), action, true, cx)
    });

    // Resolving the code action does not populate its edits. In absence of
    // edits, we must execute the given command.
    fake_server.set_request_handler::<lsp::request::CodeLensResolve, _, _>(
        |mut lens, _| async move {
            let lens_command = lens.command.as_mut().expect("should have a command");
            assert_eq!(lens_command.title, "Code lens command");
            lens_command.arguments = Some(vec![json!("the-argument")]);
            Ok(lens)
        },
    );

    // While executing the command, the language server sends the editor
    // a `workspaceEdit` request.
    fake_server
        .set_request_handler::<lsp::request::ExecuteCommand, _, _>({
            let fake = fake_server.clone();
            move |params, _| {
                assert_eq!(params.command, "_the/command");
                let fake = fake.clone();
                async move {
                    fake.server
                        .request::<lsp::request::ApplyWorkspaceEdit>(
                            lsp::ApplyWorkspaceEditParams {
                                label: None,
                                edit: lsp::WorkspaceEdit {
                                    changes: Some(
                                        [(
                                            lsp::Uri::from_file_path(path!("/dir/a.ts")).unwrap(),
                                            vec![lsp::TextEdit {
                                                range: lsp::Range::new(
                                                    lsp::Position::new(0, 0),
                                                    lsp::Position::new(0, 0),
                                                ),
                                                new_text: "X".into(),
                                            }],
                                        )]
                                        .into_iter()
                                        .collect(),
                                    ),
                                    ..lsp::WorkspaceEdit::default()
                                },
                            },
                            DEFAULT_LSP_REQUEST_TIMEOUT,
                        )
                        .await
                        .into_response()
                        .unwrap();
                    Ok(Some(json!(null)))
                }
            }
        })
        .next()
        .await;

    // Applying the code lens command returns a project transaction containing the edits
    // sent by the language server in its `workspaceEdit` request.
    let transaction = apply.await.unwrap();
    assert!(transaction.0.contains_key(&buffer));
    buffer.update(cx, |buffer, cx| {
        assert_eq!(buffer.text(), "Xa");
        buffer.undo(cx);
        assert_eq!(buffer.text(), "a");
    });

    let actions_after_edits = cx
        .update(|window, cx| project.code_actions(&buffer, anchor..anchor, window, cx))
        .unwrap()
        .await;
    assert_eq!(
        actions, actions_after_edits,
        "For the same selection, same code lens actions should be returned"
    );

    let _responses =
        fake_server.set_request_handler::<lsp::request::CodeLensRequest, _, _>(|_, _| async move {
            panic!("No more code lens requests are expected");
        });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
    });
    cx.executor().run_until_parked();
    let new_actions = cx
        .update(|window, cx| project.code_actions(&buffer, anchor..anchor, window, cx))
        .unwrap()
        .await;
    assert_eq!(
        actions, new_actions,
        "Code lens are queried for the same range and should get the same set back, but without additional LSP queries now"
    );
}

#[gpui::test]
async fn test_editor_restore_data_different_in_panes(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let main_text = r#"fn main() {
println!("1");
println!("2");
println!("3");
println!("4");
println!("5");
}"#;
    let lib_text = "mod foo {}";
    fs.insert_tree(
        path!("/a"),
        json!({
            "lib.rs": lib_text,
            "main.rs": main_text,
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let expected_ranges = vec![
        Point::new(0, 0)..Point::new(0, 0),
        Point::new(1, 0)..Point::new(1, 1),
        Point::new(2, 0)..Point::new(2, 2),
        Point::new(3, 0)..Point::new(3, 3),
    ];

    let pane_1 = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    let editor_1 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_1.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_1.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Original main.rs text on initial open",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                vec![Point::zero()..Point::zero()],
                "Default selections on initial open",
            );
        })
    });
    editor_1.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(expected_ranges.clone());
        });
    });

    let pane_2 = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_1.clone(), SplitDirection::Right, window, cx)
    });
    let editor_2 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_2.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_2.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Original main.rs text on initial open in another panel",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                vec![Point::zero()..Point::zero()],
                "Default selections on initial open in another panel",
            );
        })
    });

    editor_2.update_in(cx, |editor, window, cx| {
        editor.fold_ranges(expected_ranges.clone(), false, window, cx);
    });

    let _other_editor_1 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("lib.rs")),
                Some(pane_1.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_1
        .update_in(cx, |pane, window, cx| {
            pane.close_other_items(&CloseOtherItems::default(), None, window, cx)
        })
        .await
        .unwrap();
    drop(editor_1);
    pane_1.update(cx, |pane, cx| {
        pane.active_item()
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
            .update(cx, |editor, cx| {
                assert_eq!(
                    editor.display_text(cx),
                    lib_text,
                    "Other file should be open and active",
                );
            });
        assert_eq!(pane.items().count(), 1, "No other editors should be open");
    });

    let _other_editor_2 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("lib.rs")),
                Some(pane_2.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_2
        .update_in(cx, |pane, window, cx| {
            pane.close_other_items(&CloseOtherItems::default(), None, window, cx)
        })
        .await
        .unwrap();
    drop(editor_2);
    pane_2.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                lib_text,
                "Other file should be open and active in another panel too",
            );
        });
        assert_eq!(
            pane.items().count(),
            1,
            "No other editors should be open in another pane",
        );
    });

    let _editor_1_reopened = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_1.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    let _editor_2_reopened = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_2.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_1.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Previous editor in the 1st panel had no extra text manipulations and should get none on reopen",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                expected_ranges,
                "Previous editor in the 1st panel had selections and should get them restored on reopen",
            );
        })
    });
    pane_2.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                r#"fn main() {
⋯rintln!("1");
⋯intln!("2");
⋯ntln!("3");
println!("4");
println!("5");
}"#,
                "Previous editor in the 2nd pane had folds and should restore those on reopen in the same pane",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                vec![Point::zero()..Point::zero()],
                "Previous editor in the 2nd pane had no selections changed hence should restore none",
            );
        })
    });
}

#[gpui::test]
async fn test_editor_does_not_restore_data_when_turned_off(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let main_text = r#"fn main() {
println!("1");
println!("2");
println!("3");
println!("4");
println!("5");
}"#;
    let lib_text = "mod foo {}";
    fs.insert_tree(
        path!("/a"),
        json!({
            "lib.rs": lib_text,
            "main.rs": main_text,
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let pane = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Original main.rs text on initial open",
            );
        })
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.fold_ranges(vec![Point::new(0, 0)..Point::new(0, 0)], false, window, cx);
    });

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.workspace.restore_on_file_reopen = Some(false);
        });
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.fold_ranges(
            vec![
                Point::new(1, 0)..Point::new(1, 1),
                Point::new(2, 0)..Point::new(2, 2),
                Point::new(3, 0)..Point::new(3, 3),
            ],
            false,
            window,
            cx,
        );
    });
    pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(&CloseAllItems::default(), window, cx)
    })
    .await
    .unwrap();
    pane.update(cx, |pane, _| {
        assert!(pane.active_item().is_none());
    });
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.workspace.restore_on_file_reopen = Some(true);
        });
    });

    let _editor_reopened = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "No folds: even after enabling the restoration, previous editor's data should not be saved to be used for the restoration"
            );
        })
    });
}

struct EmptyModalView {
    focus_handle: gpui::FocusHandle,
}

impl EventEmitter<DismissEvent> for EmptyModalView {}

impl Render for EmptyModalView {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div()
    }
}

impl Focusable for EmptyModalView {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl workspace::ModalView for EmptyModalView {}

impl EmptyModalView {
    fn new(cx: &App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

#[gpui::test]
async fn test_hide_mouse_context_menu_on_modal_opened(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let buffer = cx.update(|cx| MultiBuffer::build_simple("hello world!", cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.open_context_menu(&OpenContextMenu, window, cx);
        assert!(editor.mouse_context_menu.is_some());
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView::new(cx));
    });

    cx.read(|cx| {
        assert!(editor.read(cx).mouse_context_menu.is_none());
    });
}

#[gpui::test]
async fn test_hide_pending_blame_popover_when_modal_opens(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();
    let multi_buffer = cx.update(|cx| MultiBuffer::build_simple("Buffer Contents!", cx));
    let buffer_id = multi_buffer.read_with(cx, |multi_buffer, cx| {
        multi_buffer
            .all_buffers_iter()
            .next()
            .expect("Should have at least one buffer")
            .read(cx)
            .remote_id()
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
    });

    editor.update_in(cx, |editor, _, cx| {
        editor.blame = Some(
            cx.new(|cx| GitBlame::new(editor.buffer.clone(), project.clone(), false, true, cx)),
        );
        editor.show_blame_popover(
            buffer_id,
            &::git::blame::BlameEntry {
                sha: "1b1b1b".parse().unwrap(),
                range: 0..1,
                original_line_number: 0,
                author: None,
                author_mail: None,
                author_time: None,
                author_tz: None,
                committer_name: None,
                committer_email: None,
                committer_time: None,
                committer_tz: None,
                summary: None,
                previous: None,
                filename: String::new(),
            },
            gpui::point(gpui::px(0.), gpui::px(0.)),
            false,
            cx,
        );

        assert!(editor.inline_blame_popover_show_task.is_some());
        assert!(editor.inline_blame_popover.is_none());
    });

    workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, |_, cx| EmptyModalView::new(cx));
    });

    // Toggling a modal while the blame popover task is still pending should
    // clear both the task and any rendered popover.
    editor.update_in(cx, |editor, _, _| {
        assert!(editor.inline_blame_popover.is_none());
        assert!(editor.inline_blame_popover_show_task.is_none());
    });
}

fn set_linked_edit_ranges(
    opening: (Point, Point),
    closing: (Point, Point),
    editor: &mut Editor,
    cx: &mut Context<Editor>,
) {
    let Some((buffer, _)) = editor
        .buffer
        .read(cx)
        .text_anchor_for_position(editor.selections.newest_anchor().start, cx)
    else {
        panic!("Failed to get buffer for selection position");
    };
    let buffer = buffer.read(cx);
    let buffer_id = buffer.remote_id();
    let opening_range = buffer.anchor_before(opening.0)..buffer.anchor_after(opening.1);
    let closing_range = buffer.anchor_before(closing.0)..buffer.anchor_after(closing.1);
    let mut linked_ranges = HashMap::default();
    linked_ranges.insert(buffer_id, vec![(opening_range, vec![closing_range])]);
    editor.linked_edit_ranges = LinkedEditingRanges(linked_ranges);
}

#[gpui::test]
async fn test_html_linked_edits_on_completion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.html"), Default::default())
        .await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let html_language = Arc::new(Language::new(
        LanguageConfig {
            name: "HTML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["html".to_string()],
                ..LanguageMatcher::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "<".into(),
                    end: ">".into(),
                    close: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_html::LANGUAGE.into()),
    ));
    language_registry.add(html_language);
    let mut fake_servers = language_registry.register_fake_lsp(
        "HTML",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let worktree_id = workspace.update_in(cx, |workspace, _window, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/file.html"), cx)
        })
        .await
        .unwrap();
    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("file.html")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.run_until_parked();
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("<ad></ad>", window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([Point::new(0, 3)..Point::new(0, 3)]);
        });
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 3)),
            (Point::new(0, 6), Point::new(0, 8)),
            editor,
            cx,
        );
    });
    let mut completion_handle =
        fake_server.set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "head".to_string(),
                    text_edit: Some(lsp::CompletionTextEdit::InsertAndReplace(
                        lsp::InsertReplaceEdit {
                            new_text: "head".to_string(),
                            insert: lsp::Range::new(
                                lsp::Position::new(0, 1),
                                lsp::Position::new(0, 3),
                            ),
                            replace: lsp::Range::new(
                                lsp::Position::new(0, 1),
                                lsp::Position::new(0, 3),
                            ),
                        },
                    )),
                    ..Default::default()
                },
            ])))
        });
    editor.update_in(cx, |editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    cx.run_until_parked();
    completion_handle.next().await.unwrap();
    editor.update(cx, |editor, _| {
        assert!(
            editor.context_menu_visible(),
            "Completion menu should be visible"
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.confirm_completion(&ConfirmCompletion::default(), window, cx)
    });
    cx.executor().run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.text(cx), "<head></head>");
    });
}

#[gpui::test]
async fn test_linked_edits_on_typing_punctuation(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            name: "TSX".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["tsx".to_string()],
                ..LanguageMatcher::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "<".into(),
                    end: ">".into(),
                    close: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            linked_edit_characters: HashSet::from_iter(['.']),
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // Test typing > does not extend linked pair
    cx.set_state("<divˇ<div></div>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 4)),
            (Point::new(0, 11), Point::new(0, 14)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(">", window, cx);
    });
    cx.assert_editor_state("<div>ˇ<div></div>");

    // Test typing . do extend linked pair
    cx.set_state("<Animatedˇ></Animated>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 9)),
            (Point::new(0, 12), Point::new(0, 20)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(".", window, cx);
    });
    cx.assert_editor_state("<Animated.ˇ></Animated.>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 10)),
            (Point::new(0, 13), Point::new(0, 21)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("V", window, cx);
    });
    cx.assert_editor_state("<Animated.Vˇ></Animated.V>");
}

#[gpui::test]
async fn test_linked_edits_on_typing_dot_without_language_override(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            name: "HTML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["html".to_string()],
                ..LanguageMatcher::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "<".into(),
                    end: ">".into(),
                    close: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_html::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    cx.set_state("<Tableˇ></Table>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 6)),
            (Point::new(0, 9), Point::new(0, 14)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(".", window, cx);
    });
    cx.assert_editor_state("<Table.ˇ></Table.>");
}

#[gpui::test]
async fn test_invisible_worktree_servers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "a": {
                "main.rs": "fn main() {}",
            },
            "foo": {
                "bar": {
                    "external_file.rs": "pub mod external {}",
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/root/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let _fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            ..FakeLspAdapter::default()
        },
    );
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let assert_language_servers_count =
        |expected: usize, context: &str, cx: &mut VisualTestContext| {
            project.update(cx, |project, cx| {
                let current = project
                    .lsp_store()
                    .read(cx)
                    .as_local()
                    .unwrap()
                    .language_servers
                    .len();
                assert_eq!(expected, current, "{context}");
            });
        };

    assert_language_servers_count(
        0,
        "No servers should be running before any file is open",
        cx,
    );
    let pane = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    let main_editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                "fn main() {}",
                "Original main.rs text on initial open",
            );
        });
        assert_eq!(open_editor, main_editor);
    });
    assert_language_servers_count(1, "First *.rs file starts a language server", cx);

    let external_editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from("/root/foo/bar/external_file.rs"),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .expect("opening external file")
        .downcast::<Editor>()
        .expect("downcasted external file's open element to editor");
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                "pub mod external {}",
                "External file is open now",
            );
        });
        assert_eq!(open_editor, external_editor);
    });
    assert_language_servers_count(
        1,
        "Second, external, *.rs file should join the existing server",
        cx,
    );

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(&CloseActiveItem::default(), window, cx)
    })
    .await
    .unwrap();
    pane.update_in(cx, |pane, window, cx| {
        pane.navigate_backward(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                "pub mod external {}",
                "External file is open now",
            );
        });
    });
    assert_language_servers_count(
        1,
        "After closing and reopening (with navigate back) of an external file, no extra language servers should appear",
        cx,
    );

    cx.update(|_, cx| {
        workspace::reload(cx);
    });
    assert_language_servers_count(
        1,
        "After reloading the worktree with local and external files opened, only one project should be started",
        cx,
    );
}

#[gpui::test]
async fn test_tab_in_leading_whitespace_auto_indents_for_python(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test cursor move to start of each line on tab
    // for `if`, `elif`, `else`, `while`, `with` and `for`
    cx.set_state(indoc! {"
        def main():
        ˇ    for item in items:
        ˇ        while item.active:
        ˇ            if item.value > 10:
        ˇ                continue
        ˇ            elif item.value < 0:
        ˇ                break
        ˇ            else:
        ˇ                with item.context() as ctx:
        ˇ                    yield count
        ˇ        else:
        ˇ            log('while else')
        ˇ    else:
        ˇ        log('for else')
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            ˇfor item in items:
                ˇwhile item.active:
                    ˇif item.value > 10:
                        ˇcontinue
                    ˇelif item.value < 0:
                        ˇbreak
                    ˇelse:
                        ˇwith item.context() as ctx:
                            ˇyield count
                ˇelse:
                    ˇlog('while else')
            ˇelse:
                ˇlog('for else')
    "});
    // test relative indent is preserved when tab
    // for `if`, `elif`, `else`, `while`, `with` and `for`
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
                ˇfor item in items:
                    ˇwhile item.active:
                        ˇif item.value > 10:
                            ˇcontinue
                        ˇelif item.value < 0:
                            ˇbreak
                        ˇelse:
                            ˇwith item.context() as ctx:
                                ˇyield count
                    ˇelse:
                        ˇlog('while else')
                ˇelse:
                    ˇlog('for else')
    "});

    // test cursor move to start of each line on tab
    // for `try`, `except`, `else`, `finally`, `match` and `def`
    cx.set_state(indoc! {"
        def main():
        ˇ    try:
        ˇ        fetch()
        ˇ    except ValueError:
        ˇ        handle_error()
        ˇ    else:
        ˇ        match value:
        ˇ            case _:
        ˇ    finally:
        ˇ        def status():
        ˇ            return 0
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            ˇtry:
                ˇfetch()
            ˇexcept ValueError:
                ˇhandle_error()
            ˇelse:
                ˇmatch value:
                    ˇcase _:
            ˇfinally:
                ˇdef status():
                    ˇreturn 0
    "});
    // test relative indent is preserved when tab
    // for `try`, `except`, `else`, `finally`, `match` and `def`
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
                ˇtry:
                    ˇfetch()
                ˇexcept ValueError:
                    ˇhandle_error()
                ˇelse:
                    ˇmatch value:
                        ˇcase _:
                ˇfinally:
                    ˇdef status():
                        ˇreturn 0
    "});
}

#[gpui::test]
async fn test_outdent_after_input_for_python(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test `else` auto outdents when typed inside `if` block
    cx.set_state(indoc! {"
        def main():
            if i == 2:
                return
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            if i == 2:
                return
            else:ˇ
    "});

    // test `except` auto outdents when typed inside `try` block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("except:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:ˇ
    "});

    // test `else` auto outdents when typed inside `except` block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:ˇ
    "});

    // test `finally` auto outdents when typed inside `else` block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                k = 2
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("finally:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                k = 2
            finally:ˇ
    "});

    // test `else` does not outdents when typed inside `except` block right after for block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                for i in range(n):
                    pass
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                for i in range(n):
                    pass
                else:ˇ
    "});

    // test `finally` auto outdents when typed inside `else` block right after for block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                for i in range(n):
                    pass
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("finally:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
            except:
                j = 2
            else:
                for i in range(n):
                    pass
            finally:ˇ
    "});

    // test `except` outdents to inner "try" block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
                        ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("except:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
                    except:ˇ
    "});

    // test `except` outdents to outer "try" block
    cx.set_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("except:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            try:
                i = 2
                if i == 2:
                    try:
                        i = 3
            except:ˇ
    "});

    // test `else` stays at correct indent when typed after `for` block
    cx.set_state(indoc! {"
        def main():
            for i in range(10):
                if i == 3:
                    break
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def main():
            for i in range(10):
                if i == 3:
                    break
            else:ˇ
    "});

    // test does not outdent on typing after line with square brackets
    cx.set_state(indoc! {"
        def f() -> list[str]:
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("a", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        def f() -> list[str]:
            aˇ
    "});

    // test does not outdent on typing : after case keyword
    cx.set_state(indoc! {"
        match 1:
            caseˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(":", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        match 1:
            case:ˇ
    "});
}

#[gpui::test]
async fn test_indent_on_newline_for_python(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|settings| {
        settings.defaults.extend_comment_on_newline = Some(false);
    });
    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("python", tree_sitter_python::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test correct indent after newline on comment
    cx.set_state(indoc! {"
        # COMMENT:ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        # COMMENT:
        ˇ
    "});

    // test correct indent after newline in brackets
    cx.set_state(indoc! {"
        {ˇ}
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        {
            ˇ
        }
    "});

    cx.set_state(indoc! {"
        (ˇ)
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        (
            ˇ
        )
    "});

    // do not indent after empty lists or dictionaries
    cx.set_state(indoc! {"
        a = []ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        a = []
        ˇ
    "});
}

#[gpui::test]
async fn test_python_indent_in_markdown(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language_registry = Arc::new(language::LanguageRegistry::test(cx.executor()));
    let python_lang = languages::language("python", tree_sitter_python::LANGUAGE.into());
    language_registry.add(markdown_lang());
    language_registry.add(python_lang);

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| {
        buffer.set_language_registry(language_registry);
        buffer.set_language(Some(markdown_lang()), cx);
    });

    // Test that `else:` correctly outdents to match `if:` inside the Python code block
    cx.set_state(indoc! {"
        # Heading

        ```python
        def main():
            if condition:
                pass
                ˇ
        ```
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else:", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        # Heading

        ```python
        def main():
            if condition:
                pass
            else:ˇ
        ```
    "});
}

#[gpui::test]
async fn test_tab_in_leading_whitespace_auto_indents_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test cursor move to start of each line on tab
    // for `if`, `elif`, `else`, `while`, `for`, `case` and `function`
    cx.set_state(indoc! {"
        function main() {
        ˇ    for item in $items; do
        ˇ        while [ -n \"$item\" ]; do
        ˇ            if [ \"$value\" -gt 10 ]; then
        ˇ                continue
        ˇ            elif [ \"$value\" -lt 0 ]; then
        ˇ                break
        ˇ            else
        ˇ                echo \"$item\"
        ˇ            fi
        ˇ        done
        ˇ    done
        ˇ}
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function main() {
            ˇfor item in $items; do
                ˇwhile [ -n \"$item\" ]; do
                    ˇif [ \"$value\" -gt 10 ]; then
                        ˇcontinue
                    ˇelif [ \"$value\" -lt 0 ]; then
                        ˇbreak
                    ˇelse
                        ˇecho \"$item\"
                    ˇfi
                ˇdone
            ˇdone
        ˇ}
    "});
    // test relative indent is preserved when tab
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function main() {
                ˇfor item in $items; do
                    ˇwhile [ -n \"$item\" ]; do
                        ˇif [ \"$value\" -gt 10 ]; then
                            ˇcontinue
                        ˇelif [ \"$value\" -lt 0 ]; then
                            ˇbreak
                        ˇelse
                            ˇecho \"$item\"
                        ˇfi
                    ˇdone
                ˇdone
            ˇ}
    "});

    // test cursor move to start of each line on tab
    // for `case` statement with patterns
    cx.set_state(indoc! {"
        function handle() {
        ˇ    case \"$1\" in
        ˇ        start)
        ˇ            echo \"a\"
        ˇ            ;;
        ˇ        stop)
        ˇ            echo \"b\"
        ˇ            ;;
        ˇ        *)
        ˇ            echo \"c\"
        ˇ            ;;
        ˇ    esac
        ˇ}
    "});
    cx.update_editor(|e, window, cx| e.tab(&Tab, window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function handle() {
            ˇcase \"$1\" in
                ˇstart)
                    ˇecho \"a\"
                    ˇ;;
                ˇstop)
                    ˇecho \"b\"
                    ˇ;;
                ˇ*)
                    ˇecho \"c\"
                    ˇ;;
            ˇesac
        ˇ}
    "});
}

#[gpui::test]
async fn test_indent_after_input_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test indents on comment insert
    cx.set_state(indoc! {"
        function main() {
        ˇ    for item in $items; do
        ˇ        while [ -n \"$item\" ]; do
        ˇ            if [ \"$value\" -gt 10 ]; then
        ˇ                continue
        ˇ            elif [ \"$value\" -lt 0 ]; then
        ˇ                break
        ˇ            else
        ˇ                echo \"$item\"
        ˇ            fi
        ˇ        done
        ˇ    done
        ˇ}
    "});
    cx.update_editor(|e, window, cx| e.handle_input("#", window, cx));
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function main() {
        #ˇ    for item in $items; do
        #ˇ        while [ -n \"$item\" ]; do
        #ˇ            if [ \"$value\" -gt 10 ]; then
        #ˇ                continue
        #ˇ            elif [ \"$value\" -lt 0 ]; then
        #ˇ                break
        #ˇ            else
        #ˇ                echo \"$item\"
        #ˇ            fi
        #ˇ        done
        #ˇ    done
        #ˇ}
    "});
}

#[gpui::test]
async fn test_outdent_after_input_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test `else` auto outdents when typed inside `if` block
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("else", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        elseˇ
    "});

    // test `elif` auto outdents when typed inside `if` block
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("elif", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        elifˇ
    "});

    // test `fi` auto outdents when typed inside `else` block
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        else
            echo \"bar baz\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("fi", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"foo bar\"
        else
            echo \"bar baz\"
        fiˇ
    "});

    // test `done` auto outdents when typed inside `while` block
    cx.set_state(indoc! {"
        while read line; do
            echo \"$line\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("done", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        while read line; do
            echo \"$line\"
        doneˇ
    "});

    // test `done` auto outdents when typed inside `for` block
    cx.set_state(indoc! {"
        for file in *.txt; do
            cat \"$file\"
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("done", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        for file in *.txt; do
            cat \"$file\"
        doneˇ
    "});

    // test `esac` auto outdents when typed inside `case` block
    cx.set_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
            stop)
                echo \"bar baz\"
                ;;
            ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("esac", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
            stop)
                echo \"bar baz\"
                ;;
        esacˇ
    "});

    // test `*)` auto outdents when typed inside `case` block
    cx.set_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("*)", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                echo \"foo bar\"
                ;;
            *)ˇ
    "});

    // test `fi` outdents to correct level with nested if blocks
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"outer if\"
            if [ \"$2\" = \"debug\" ]; then
                echo \"inner if\"
                ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("fi", window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
            echo \"outer if\"
            if [ \"$2\" = \"debug\" ]; then
                echo \"inner if\"
            fiˇ
    "});
}

#[gpui::test]
async fn test_indent_on_newline_for_bash(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|settings| {
        settings.defaults.extend_comment_on_newline = Some(false);
    });
    let mut cx = EditorTestContext::new(cx).await;
    let language = languages::language("bash", tree_sitter_bash::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // test correct indent after newline on comment
    cx.set_state(indoc! {"
        # COMMENT:ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        # COMMENT:
        ˇ
    "});

    // test correct indent after newline after `then`
    cx.set_state(indoc! {"

        if [ \"$1\" = \"test\" ]; thenˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"

        if [ \"$1\" = \"test\" ]; then
            ˇ
    "});

    // test correct indent after newline after `else`
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        elseˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        else
            ˇ
    "});

    // test correct indent after newline after `elif`
    cx.set_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        elifˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        if [ \"$1\" = \"test\" ]; then
        elif
            ˇ
    "});

    // test correct indent after newline after `do`
    cx.set_state(indoc! {"
        for file in *.txt; doˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        for file in *.txt; do
            ˇ
    "});

    // test correct indent after newline after case pattern
    cx.set_state(indoc! {"
        case \"$1\" in
            start)ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                ˇ
    "});

    // test correct indent after newline after case pattern
    cx.set_state(indoc! {"
        case \"$1\" in
            start)
                ;;
            *)ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        case \"$1\" in
            start)
                ;;
            *)
                ˇ
    "});

    // test correct indent after newline after function opening brace
    cx.set_state(indoc! {"
        function test() {ˇ}
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        function test() {
            ˇ
        }
    "});

    // test no extra indent after semicolon on same line
    cx.set_state(indoc! {"
        echo \"test\";ˇ
    "});
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.wait_for_autoindent_applied().await;
    cx.assert_editor_state(indoc! {"
        echo \"test\";
        ˇ
    "});
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

#[gpui::test]
async fn test_mixed_completions_with_multi_word_snippet(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;
    cx.lsp
        .set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "unsafe".into(),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range {
                            start: lsp::Position {
                                line: 0,
                                character: 9,
                            },
                            end: lsp::Position {
                                line: 0,
                                character: 11,
                            },
                        },
                        new_text: "unsafe".to_string(),
                    })),
                    insert_text_mode: Some(lsp::InsertTextMode::AS_IS),
                    ..Default::default()
                },
            ])))
        });

    cx.update_editor(|editor, _, cx| {
        editor.project().unwrap().update(cx, |project, cx| {
            project.snippets().update(cx, |snippets, _cx| {
                snippets.add_snippet_for_test(
                    None,
                    PathBuf::from("test_snippets.json"),
                    vec![
                        Arc::new(project::snippet_provider::Snippet {
                            prefix: vec![
                                "unlimited word count".to_string(),
                                "unlimit word count".to_string(),
                                "unlimited unknown".to_string(),
                            ],
                            body: "this is many words".to_string(),
                            description: Some("description".to_string()),
                            name: "multi-word snippet test".to_string(),
                        }),
                        Arc::new(project::snippet_provider::Snippet {
                            prefix: vec!["unsnip".to_string(), "@few".to_string()],
                            body: "fewer words".to_string(),
                            description: Some("alt description".to_string()),
                            name: "other name".to_string(),
                        }),
                        Arc::new(project::snippet_provider::Snippet {
                            prefix: vec!["ab aa".to_string()],
                            body: "abcd".to_string(),
                            description: None,
                            name: "alphabet".to_string(),
                        }),
                    ],
                );
            });
        })
    });

    let get_completions = |cx: &mut EditorLspTestContext| {
        cx.update_editor(|editor, _, _| match &*editor.context_menu.borrow() {
            Some(CodeContextMenu::Completions(context_menu)) => {
                let entries = context_menu.entries.borrow();
                entries
                    .iter()
                    .filter_map(|entry| entry.as_match().map(|m| m.string.clone()))
                    .collect_vec()
            }
            _ => vec![],
        })
    };

    // snippets:
    //  @foo
    //  foo bar
    //
    // when typing:
    //
    // when typing:
    //  - if I type a symbol "open the completions with snippets only"
    //  - if I type a word character "open the completions menu" (if it had been open snippets only, clear it out)
    //
    // stuff we need:
    //  - filtering logic change?
    //  - remember how far back the completion started.

    let test_cases: &[(&str, &[&str])] = &[
        (
            "un",
            &[
                "unsafe",
                "unlimit word count",
                "unlimited unknown",
                "unlimited word count",
                "unsnip",
            ],
        ),
        (
            "u ",
            &[
                "unlimit word count",
                "unlimited unknown",
                "unlimited word count",
            ],
        ),
        ("u a", &["ab aa", "unsafe"]), // unsAfe
        (
            "u u",
            &[
                "unsafe",
                "unlimit word count",
                "unlimited unknown", // ranked highest among snippets
                "unlimited word count",
                "unsnip",
            ],
        ),
        ("uw c", &["unlimit word count", "unlimited word count"]),
        (
            "u w",
            &[
                "unlimit word count",
                "unlimited word count",
                "unlimited unknown",
            ],
        ),
        ("u w ", &["unlimit word count", "unlimited word count"]),
        (
            "u ",
            &[
                "unlimit word count",
                "unlimited unknown",
                "unlimited word count",
            ],
        ),
        ("wor", &[]),
        ("uf", &["unsafe"]),
        ("af", &["unsafe"]),
        ("afu", &[]),
        (
            "ue",
            &["unsafe", "unlimited unknown", "unlimited word count"],
        ),
        ("@", &["@few"]),
        ("@few", &["@few"]),
        ("@ ", &[]),
        ("a@", &["@few"]),
        ("a@f", &["@few", "unsafe"]),
        ("a@fw", &["@few"]),
        ("a", &["ab aa", "unsafe"]),
        ("aa", &["ab aa"]),
        ("aaa", &["ab aa"]),
        ("ab", &["ab aa"]),
        ("ab ", &["ab aa"]),
        ("ab a", &["ab aa", "unsafe"]),
        ("ab ab", &["ab aa"]),
        ("ab ab aa", &["ab aa"]),
    ];

    for &(input_to_simulate, expected_completions) in test_cases {
        cx.set_state("fn a() { ˇ }\n");
        for c in input_to_simulate.split("") {
            cx.simulate_input(c);
            cx.run_until_parked();
        }
        let expected_completions = expected_completions
            .iter()
            .map(|s| s.to_string())
            .collect_vec();
        assert_eq!(
            get_completions(&mut cx),
            expected_completions,
            "< actual / expected >, input = {input_to_simulate:?}",
        );
    }
}

/// Handle completion request passing a marked string specifying where the completion
/// should be triggered from using '|' character, what range should be replaced, and what completions
/// should be returned using '<' and '>' to delimit the range.
///
/// Also see `handle_completion_request_with_insert_and_replace`.
#[track_caller]
pub fn handle_completion_request(
    marked_string: &str,
    completions: Vec<&'static str>,
    is_incomplete: bool,
    counter: Arc<AtomicUsize>,
    cx: &mut EditorLspTestContext,
) -> impl Future<Output = ()> {
    let complete_from_marker: TextRangeMarker = '|'.into();
    let replace_range_marker: TextRangeMarker = ('<', '>').into();
    let (_, mut marked_ranges) = marked_text_ranges_by(
        marked_string,
        vec![complete_from_marker.clone(), replace_range_marker.clone()],
    );

    let complete_from_position = cx.to_lsp(MultiBufferOffset(
        marked_ranges.remove(&complete_from_marker).unwrap()[0].start,
    ));
    let range = marked_ranges.remove(&replace_range_marker).unwrap()[0].clone();
    let replace_range =
        cx.to_lsp_range(MultiBufferOffset(range.start)..MultiBufferOffset(range.end));

    let mut request =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |url, params, _| {
            let completions = completions.clone();
            counter.fetch_add(1, atomic::Ordering::Release);
            async move {
                assert_eq!(params.text_document_position.text_document.uri, url.clone());
                assert_eq!(
                    params.text_document_position.position,
                    complete_from_position
                );
                Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                    is_incomplete,
                    item_defaults: None,
                    items: completions
                        .iter()
                        .map(|completion_text| lsp::CompletionItem {
                            label: completion_text.to_string(),
                            text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                                range: replace_range,
                                new_text: completion_text.to_string(),
                            })),
                            ..Default::default()
                        })
                        .collect(),
                })))
            }
        });

    async move {
        request.next().await;
    }
}

/// Similar to `handle_completion_request`, but a [`CompletionTextEdit::InsertAndReplace`] will be
/// given instead, which also contains an `insert` range.
///
/// This function uses markers to define ranges:
/// - `|` marks the cursor position
/// - `<>` marks the replace range
/// - `[]` marks the insert range (optional, defaults to `replace_range.start..cursor_pos`which is what Rust-Analyzer provides)
pub fn handle_completion_request_with_insert_and_replace(
    cx: &mut EditorLspTestContext,
    marked_string: &str,
    completions: Vec<(&'static str, &'static str)>, // (label, new_text)
    counter: Arc<AtomicUsize>,
) -> impl Future<Output = ()> {
    let complete_from_marker: TextRangeMarker = '|'.into();
    let replace_range_marker: TextRangeMarker = ('<', '>').into();
    let insert_range_marker: TextRangeMarker = ('{', '}').into();

    let (_, mut marked_ranges) = marked_text_ranges_by(
        marked_string,
        vec![
            complete_from_marker.clone(),
            replace_range_marker.clone(),
            insert_range_marker.clone(),
        ],
    );

    let complete_from_position = cx.to_lsp(MultiBufferOffset(
        marked_ranges.remove(&complete_from_marker).unwrap()[0].start,
    ));
    let range = marked_ranges.remove(&replace_range_marker).unwrap()[0].clone();
    let replace_range =
        cx.to_lsp_range(MultiBufferOffset(range.start)..MultiBufferOffset(range.end));

    let insert_range = match marked_ranges.remove(&insert_range_marker) {
        Some(ranges) if !ranges.is_empty() => {
            let range1 = ranges[0].clone();
            cx.to_lsp_range(MultiBufferOffset(range1.start)..MultiBufferOffset(range1.end))
        }
        _ => lsp::Range {
            start: replace_range.start,
            end: complete_from_position,
        },
    };

    let mut request =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |url, params, _| {
            let completions = completions.clone();
            counter.fetch_add(1, atomic::Ordering::Release);
            async move {
                assert_eq!(params.text_document_position.text_document.uri, url.clone());
                assert_eq!(
                    params.text_document_position.position, complete_from_position,
                    "marker `|` position doesn't match",
                );
                Ok(Some(lsp::CompletionResponse::Array(
                    completions
                        .iter()
                        .map(|(label, new_text)| lsp::CompletionItem {
                            label: label.to_string(),
                            text_edit: Some(lsp::CompletionTextEdit::InsertAndReplace(
                                lsp::InsertReplaceEdit {
                                    insert: insert_range,
                                    replace: replace_range,
                                    new_text: new_text.to_string(),
                                },
                            )),
                            ..Default::default()
                        })
                        .collect(),
                )))
            }
        });

    async move {
        request.next().await;
    }
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

#[gpui::test(iterations = 10)]
async fn test_pulling_diagnostics(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let diagnostic_requests = Arc::new(AtomicUsize::new(0));
    let counter = diagnostic_requests.clone();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "first.rs": "fn main() { let a = 5; }",
            "second.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                diagnostic_provider: Some(lsp::DiagnosticServerCapabilities::Options(
                    lsp::DiagnosticOptions {
                        identifier: None,
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        work_done_progress_options: Default::default(),
                    },
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/a/first.rs")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let fake_server = fake_servers.next().await.unwrap();
    let server_id = fake_server.server.server_id();
    let mut first_request = fake_server
        .set_request_handler::<lsp::request::DocumentDiagnosticRequest, _, _>(move |params, _| {
            let new_result_id = counter.fetch_add(1, atomic::Ordering::Release) + 1;
            let result_id = Some(new_result_id.to_string());
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/first.rs")).unwrap()
            );
            async move {
                Ok(lsp::DocumentDiagnosticReportResult::Report(
                    lsp::DocumentDiagnosticReport::Full(lsp::RelatedFullDocumentDiagnosticReport {
                        related_documents: None,
                        full_document_diagnostic_report: lsp::FullDocumentDiagnosticReport {
                            items: Vec::new(),
                            result_id,
                        },
                    }),
                ))
            }
        });

    let ensure_result_id = |expected_result_id: Option<SharedString>, cx: &mut TestAppContext| {
        project.update(cx, |project, cx| {
            let buffer_id = editor
                .read(cx)
                .buffer()
                .read(cx)
                .as_singleton()
                .expect("created a singleton buffer")
                .read(cx)
                .remote_id();
            let buffer_result_id = project
                .lsp_store()
                .read(cx)
                .result_id_for_buffer_pull(server_id, buffer_id, &None, cx);
            assert_eq!(expected_result_id, buffer_result_id);
        });
    };

    ensure_result_id(None, cx);
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();
    assert_eq!(
        diagnostic_requests.load(atomic::Ordering::Acquire),
        1,
        "Opening file should trigger diagnostic request"
    );
    first_request
        .next()
        .await
        .expect("should have sent the first diagnostics pull request");
    ensure_result_id(Some(SharedString::new_static("1")), cx);

    // Editing should trigger diagnostics
    editor.update_in(cx, |editor, window, cx| {
        editor.handle_input("2", window, cx)
    });
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();
    assert_eq!(
        diagnostic_requests.load(atomic::Ordering::Acquire),
        2,
        "Editing should trigger diagnostic request"
    );
    ensure_result_id(Some(SharedString::new_static("2")), cx);

    // Moving cursor should not trigger diagnostic request
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)])
        });
    });
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();
    assert_eq!(
        diagnostic_requests.load(atomic::Ordering::Acquire),
        2,
        "Cursor movement should not trigger diagnostic request"
    );
    ensure_result_id(Some(SharedString::new_static("2")), cx);
    // Multiple rapid edits should be debounced
    for _ in 0..5 {
        editor.update_in(cx, |editor, window, cx| {
            editor.handle_input("x", window, cx)
        });
    }
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.executor().run_until_parked();

    let final_requests = diagnostic_requests.load(atomic::Ordering::Acquire);
    assert!(
        final_requests <= 4,
        "Multiple rapid edits should be debounced (got {final_requests} requests)",
    );
    ensure_result_id(Some(SharedString::new(final_requests.to_string())), cx);
}

#[gpui::test]
async fn test_add_selection_after_moving_with_multiple_cursors(cx: &mut TestAppContext) {
    // Regression test for issue #11671
    // Previously, adding a cursor after moving multiple cursors would reset
    // the cursor count instead of adding to the existing cursors.
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // Create a simple buffer with cursor at start
    cx.set_state(indoc! {"
        ˇaaaa
        bbbb
        cccc
        dddd
        eeee
        ffff
        gggg
        hhhh"});

    // Add 2 cursors below (so we have 3 total)
    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // Verify we have 3 cursors
    let initial_count = cx.update_editor(|editor, _, _| editor.selections.count());
    assert_eq!(
        initial_count, 3,
        "Should have 3 cursors after adding 2 below"
    );

    // Move down one line
    cx.update_editor(|editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
    });

    // Add another cursor below
    cx.update_editor(|editor, window, cx| {
        editor.add_selection_below(&Default::default(), window, cx);
    });

    // Should now have 4 cursors (3 original + 1 new)
    let final_count = cx.update_editor(|editor, _, _| editor.selections.count());
    assert_eq!(
        final_count, 4,
        "Should have 4 cursors after moving and adding another"
    );
}

#[gpui::test]
async fn test_add_selection_skip_soft_wrap_option(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc!(
        r#"ˇThis is a very long line that will be wrapped when soft wrapping is enabled
           Second line here"#
    ));

    cx.update_editor(|editor, window, cx| {
        // Enable soft wrapping with a narrow width to force soft wrapping and
        // confirm that more than 2 rows are being displayed.
        editor.set_wrap_width(Some(100.0.into()), cx);
        assert!(editor.display_text(cx).lines().count() > 2);

        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(8), 0)..DisplayPoint::new(DisplayRow(8), 0),
            ]
        );

        editor.add_selection_above(
            &AddSelectionAbove {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: false,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
            ]
        );

        editor.add_selection_above(
            &AddSelectionAbove {
                skip_soft_wrap: false,
            },
            window,
            cx,
        );

        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });

    // Set up text where selections are in the middle of a soft-wrapped line.
    // When adding selection below with `skip_soft_wrap` set to `true`, the new
    // selection should be at the same buffer column, not the same pixel
    // position.
    cx.set_state(indoc!(
        r#"1. Very long line to show «howˇ» a wrapped line would look
           2. Very long line to show how a wrapped line would look"#
    ));

    cx.update_editor(|editor, window, cx| {
        // Enable soft wrapping with a narrow width to force soft wrapping and
        // confirm that more than 2 rows are being displayed.
        editor.set_wrap_width(Some(100.0.into()), cx);
        assert!(editor.display_text(cx).lines().count() > 2);

        editor.add_selection_below(
            &AddSelectionBelow {
                skip_soft_wrap: true,
            },
            window,
            cx,
        );

        // Assert that there's now 2 selections, both selecting the same column
        // range in the buffer row.
        let display_map = editor.display_map.update(cx, |map, cx| map.snapshot(cx));
        let selections = editor.selections.all::<Point>(&display_map);
        assert_eq!(selections.len(), 2);
        assert_eq!(selections[0].start.column, selections[1].start.column);
        assert_eq!(selections[0].end.column, selections[1].end.column);
    });
}

#[gpui::test]
async fn test_insert_snippet(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.update_editor(|editor, _, cx| {
        editor.project().unwrap().update(cx, |project, cx| {
            project.snippets().update(cx, |snippets, _cx| {
                let snippet = project::snippet_provider::Snippet {
                    prefix: vec![], // no prefix needed!
                    body: "an Unspecified".to_string(),
                    description: Some("shhhh it's a secret".to_string()),
                    name: "super secret snippet".to_string(),
                };
                snippets.add_snippet_for_test(
                    None,
                    PathBuf::from("test_snippets.json"),
                    vec![Arc::new(snippet)],
                );

                let snippet = project::snippet_provider::Snippet {
                    prefix: vec![], // no prefix needed!
                    body: " Location".to_string(),
                    description: Some("the word 'location'".to_string()),
                    name: "location word".to_string(),
                };
                snippets.add_snippet_for_test(
                    Some("Markdown".to_string()),
                    PathBuf::from("test_snippets.json"),
                    vec![Arc::new(snippet)],
                );
            });
        })
    });

    cx.set_state(indoc!(r#"First cursor at ˇ and second cursor at ˇ"#));

    cx.update_editor(|editor, window, cx| {
        editor.insert_snippet_at_selections(
            &InsertSnippet {
                language: None,
                name: Some("super secret snippet".to_string()),
                snippet: None,
            },
            window,
            cx,
        );

        // Language is specified in the action,
        // so the buffer language does not need to match
        editor.insert_snippet_at_selections(
            &InsertSnippet {
                language: Some("Markdown".to_string()),
                name: Some("location word".to_string()),
                snippet: None,
            },
            window,
            cx,
        );

        editor.insert_snippet_at_selections(
            &InsertSnippet {
                language: None,
                name: None,
                snippet: Some("$0 after".to_string()),
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(
        r#"First cursor at an Unspecified Locationˇ after and second cursor at an Unspecified Locationˇ after"#,
    );
}

#[gpui::test]
async fn test_inlay_hints_request_timeout(cx: &mut TestAppContext) {
    use crate::inlays::inlay_hints::InlayHintRefreshReason;
    use crate::inlays::inlay_hints::tests::{cached_hint_labels, init_test, visible_hint_labels};
    use settings::InlayHintSettingsContent;
    use std::sync::atomic::AtomicU32;
    use std::time::Duration;

    const BASE_TIMEOUT_SECS: u64 = 1;

    let request_count = Arc::new(AtomicU32::new(0));
    let closure_request_count = request_count.clone();

    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            enabled: Some(true),
            ..InlayHintSettingsContent::default()
        })
    });
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, &|settings: &mut SettingsContent| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    request_timeout: Some(BASE_TIMEOUT_SECS),
                    button: Some(true),
                    notifications: None,
                    semantic_token_rules: None,
                });
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let a = 5; }",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let request_count = closure_request_count.clone();
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, cx| {
                        let request_count = request_count.clone();
                        async move {
                            cx.background_executor()
                                .timer(Duration::from_secs(BASE_TIMEOUT_SECS * 2))
                                .await;
                            let count = request_count.fetch_add(1, atomic::Ordering::Release) + 1;
                            assert_eq!(
                                params.text_document.uri,
                                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                            );
                            Ok(Some(vec![lsp::InlayHint {
                                position: lsp::Position::new(0, 1),
                                label: lsp::InlayHintLabel::String(count.to_string()),
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
            })),
            ..FakeLspAdapter::default()
        },
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor = cx.add_window(|window, cx| Editor::for_buffer(buffer, Some(project), window, cx));

    cx.executor().run_until_parked();
    let fake_server = fake_servers.next().await.unwrap();

    cx.executor()
        .advance_clock(Duration::from_secs(BASE_TIMEOUT_SECS) + Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert!(
                cached_hint_labels(editor, cx).is_empty(),
                "First request should time out, no hints cached"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(
                InlayHintRefreshReason::RefreshRequested {
                    server_id: fake_server.server.server_id(),
                    request_id: Some(1),
                },
                cx,
            );
        })
        .unwrap();
    cx.executor()
        .advance_clock(Duration::from_secs(BASE_TIMEOUT_SECS) + Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert!(
                cached_hint_labels(editor, cx).is_empty(),
                "Second request should also time out with BASE_TIMEOUT, no hints cached"
            );
        })
        .unwrap();

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    request_timeout: Some(BASE_TIMEOUT_SECS * 4),
                    button: Some(true),
                    notifications: None,
                    semantic_token_rules: None,
                });
            });
        });
    });
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(
                InlayHintRefreshReason::RefreshRequested {
                    server_id: fake_server.server.server_id(),
                    request_id: Some(2),
                },
                cx,
            );
        })
        .unwrap();
    cx.executor()
        .advance_clock(Duration::from_secs(BASE_TIMEOUT_SECS * 4) + Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "With extended timeout (BASE * 4), hints should arrive successfully"
            );
            assert_eq!(vec!["1".to_string()], visible_hint_labels(editor, cx));
        })
        .unwrap();
}

#[gpui::test]
async fn test_click_on_parameter_inlay_hint_places_cursor_correctly(cx: &mut TestAppContext) {
    use crate::inlays::inlay_hints::tests::{cached_hint_labels, visible_hint_labels};

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            inlay_hint_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, &|settings: &mut SettingsContent| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        show_parameter_hints: Some(true),
                        show_type_hints: Some(true),
                        edit_debounce_ms: Some(0),
                        scroll_debounce_ms: Some(0),
                        ..Default::default()
                    })
            });
        });
    });

    cx.set_state("fn foo(value: i32) {} fn main() { foo(ˇ42); }");

    // Buffer: `fn foo(value: i32) {} fn main() { foo(42); }`
    // The parameter hint "value:" appears before "42"
    let hint_start_offset = cx.ranges("fn foo(value: i32) {} fn main() { foo(ˇ42); }")[0].start;
    let hint_position = cx.to_lsp(MultiBufferOffset(hint_start_offset));
    let hint_label = "value:";
    let expected_uri = cx.buffer_lsp_url.clone();
    cx.lsp
        .set_request_handler::<lsp::request::InlayHintRequest, _, _>(move |params, _| {
            let expected_uri = expected_uri.clone();
            async move {
                assert_eq!(params.text_document.uri, expected_uri);
                Ok(Some(vec![lsp::InlayHint {
                    position: hint_position,
                    label: lsp::InlayHintLabel::String(hint_label.to_string()),
                    kind: Some(lsp::InlayHintKind::PARAMETER),
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: Some(true),
                    data: None,
                }]))
            }
        })
        .next()
        .await;
    cx.background_executor.run_until_parked();

    cx.update_editor(|editor, _window, cx| {
        let expected_labels = vec!["value: ".to_string()];
        assert_eq!(expected_labels, cached_hint_labels(editor, cx));
        assert_eq!(expected_labels, visible_hint_labels(editor, cx));
    });

    // The cursor is at `4` in `42`. The parameter hint "value: " appears just
    // before it in display space. We'll click a few characters to the left of
    // the cursor position to land inside the inlay hint text.
    let cursor_display_point = cx.update_editor(|editor, _window, cx| {
        editor
            .selections
            .newest_display(&editor.display_snapshot(cx))
            .head()
    });
    let cursor_pixel = cx.pixel_position_for(cursor_display_point);
    let em_width =
        cx.update_editor(|editor, _, _| editor.last_position_map.as_ref().unwrap().em_layout_width);
    // Click 3 characters to the left of the cursor, which lands inside the
    // "value: " inlay hint text.
    let click_position = gpui::Point {
        x: cursor_pixel.x - em_width * 3.0,
        y: cursor_pixel.y,
    };
    cx.simulate_click(click_position, Modifiers::none());
    cx.background_executor.run_until_parked();

    // The cursor should be placed after the `(`, at the `4` in `42`,
    // NOT before the `(`.
    cx.assert_editor_state("fn foo(value: i32) {} fn main() { foo(ˇ42); }");
}

#[gpui::test]
async fn test_newline_replacement_in_single_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let (editor, cx) = cx.add_window_view(Editor::single_line);
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("oops\n\nwow\n", window, cx)
    });
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.display_text(cx), "oops⋯⋯wow⋯");
    });
    editor.update(cx, |editor, cx| {
        editor.edit([(MultiBufferOffset(3)..MultiBufferOffset(5), "")], cx)
    });
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.display_text(cx), "oop⋯wow⋯");
    });
}

#[gpui::test]
async fn test_non_utf_8_opens(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    cx.update(|cx| {
        register_project_item::<Editor>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root1", json!({})).await;
    fs.insert_file("/root1/one.pdf", vec![0xff, 0xfe, 0xfd])
        .await;

    let project = Project::test(fs, ["/root1".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let handle = workspace
        .update_in(cx, |workspace, window, cx| {
            let project_path = (worktree_id, rel_path("one.pdf"));
            workspace.open_path(project_path, None, true, window, cx)
        })
        .await
        .unwrap();
    // The test file content `vec![0xff, 0xfe, ...]` starts with a UTF-16 LE BOM.
    // Previously, this fell back to `InvalidItemView` because it wasn't valid UTF-8.
    // With auto-detection enabled, this is now recognized as UTF-16 and opens in the Editor.
    assert_eq!(handle.to_any_view().entity_type(), TypeId::of::<Editor>());
}

#[gpui::test]
async fn test_select_next_prev_syntax_node(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    // Test hierarchical sibling navigation
    let text = r#"
        fn outer() {
            if condition {
                let a = 1;
            }
            let b = 2;
        }

        fn another() {
            let c = 3;
        }
    "#;

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    // Wait for parsing to complete
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        // Start by selecting "let a = 1;" inside the if block
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 16)..DisplayPoint::new(DisplayRow(3), 26)
            ]);
        });

        let initial_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));
        assert_eq!(initial_selection.len(), 1, "Should have one selection");

        // Test select next sibling - should move up levels to find the next sibling
        // Since "let a = 1;" has no siblings in the if block, it should move up
        // to find "let b = 2;" which is a sibling of the if block
        editor.select_next_syntax_node(&SelectNextSyntaxNode, window, cx);
        let next_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));

        // Should have a selection and it should be different from the initial
        assert_eq!(
            next_selection.len(),
            1,
            "Should have one selection after next"
        );
        assert_ne!(
            next_selection[0], initial_selection[0],
            "Next sibling selection should be different"
        );

        // Test hierarchical navigation by going to the end of the current function
        // and trying to navigate to the next function
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(5), 12)..DisplayPoint::new(DisplayRow(5), 22)
            ]);
        });

        editor.select_next_syntax_node(&SelectNextSyntaxNode, window, cx);
        let function_next_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));

        // Should move to the next function
        assert_eq!(
            function_next_selection.len(),
            1,
            "Should have one selection after function next"
        );

        // Test select previous sibling navigation
        editor.select_prev_syntax_node(&SelectPreviousSyntaxNode, window, cx);
        let prev_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));

        // Should have a selection and it should be different
        assert_eq!(
            prev_selection.len(),
            1,
            "Should have one selection after prev"
        );
        assert_ne!(
            prev_selection[0], function_next_selection[0],
            "Previous sibling selection should be different from next"
        );
    });
}

#[gpui::test]
async fn test_next_prev_document_highlight(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        "let ˇvariable = 42;
let another = variable + 1;
let result = variable * 2;",
    );

    // Set up document highlights manually (simulating LSP response)
    cx.update_editor(|editor, _window, cx| {
        let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);

        // Create highlights for "variable" occurrences
        let highlight_ranges = [
            Point::new(0, 4)..Point::new(0, 12),  // First "variable"
            Point::new(1, 14)..Point::new(1, 22), // Second "variable"
            Point::new(2, 13)..Point::new(2, 21), // Third "variable"
        ];

        let anchor_ranges: Vec<_> = highlight_ranges
            .iter()
            .map(|range| range.clone().to_anchors(&buffer_snapshot))
            .collect();

        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &anchor_ranges,
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
    });

    // Go to next highlight - should move to second "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_document_highlight(&GoToNextDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = ˇvariable + 1;
let result = variable * 2;",
    );

    // Go to next highlight - should move to third "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_document_highlight(&GoToNextDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = variable + 1;
let result = ˇvariable * 2;",
    );

    // Go to next highlight - should stay at third "variable" (no wrap-around)
    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_document_highlight(&GoToNextDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = variable + 1;
let result = ˇvariable * 2;",
    );

    // Now test going backwards from third position
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_document_highlight(&GoToPreviousDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = ˇvariable + 1;
let result = variable * 2;",
    );

    // Go to previous highlight - should move to first "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_document_highlight(&GoToPreviousDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let ˇvariable = 42;
let another = variable + 1;
let result = variable * 2;",
    );

    // Go to previous highlight - should stay on first "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_document_highlight(&GoToPreviousDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let ˇvariable = 42;
let another = variable + 1;
let result = variable * 2;",
    );
}

#[gpui::test]
async fn test_paste_url_from_other_app_creates_markdown_link_over_selected_text(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("Hello, «editorˇ».\nMav is «ˇgreat» (see this link: ˇ)");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "Hello, [editor]({url})ˇ.\nMav is [great]({url})ˇ (see this link: {url}ˇ)"
    ));
}

#[gpui::test]
async fn test_markdown_indents(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let markdown_language = languages::language("markdown", tree_sitter_md::LANGUAGE.into());
    let mut cx = EditorTestContext::new(cx).await;

    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));

    // Case 1: Test if adding a character with multi cursors preserves nested list indents
    cx.set_state(&indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [ˇ] Item 2
            - [ˇ] Item 2.a
            - [ˇ] Item 2.b
        "
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("x", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [xˇ] Item 2
            - [xˇ] Item 2.a
            - [xˇ] Item 2.b
        "
    });

    // Case 2: Test adding new line after nested list continues the list with unchecked task
    cx.set_state(&indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [x] Item 2
            - [x] Item 2.a
            - [x] Item 2.bˇ"
    });
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [x] Item 2
            - [x] Item 2.a
            - [x] Item 2.b
            - [ ] ˇ"
    });

    // Case 3: Test adding content to continued list item
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Item 2.c", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        - [ ] Item 1
            - [ ] Item 1.a
        - [x] Item 2
            - [x] Item 2.a
            - [x] Item 2.b
            - [ ] Item 2.cˇ"
    });

    // Case 4: Test adding new line after nested ordered list continues with next number
    cx.set_state(indoc! {"
        1. Item 1
            1. Item 1.a
        2. Item 2
            1. Item 2.a
            2. Item 2.bˇ"
    });
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        1. Item 1
            1. Item 1.a
        2. Item 2
            1. Item 2.a
            2. Item 2.b
            3. ˇ"
    });

    // Case 5: Adding content to continued ordered list item
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("Item 2.c", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        1. Item 1
            1. Item 1.a
        2. Item 2
            1. Item 2.a
            2. Item 2.b
            3. Item 2.cˇ"
    });

    // Case 6: Test adding new line after nested ordered list preserves indent of previous line
    cx.set_state(indoc! {"
        - Item 1
            - Item 1.a
            - Item 1.a
        ˇ"});
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("-", window, cx);
    });
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        - Item 1
            - Item 1.a
            - Item 1.a
        -ˇ"});

    // Case 7: Test blockquote newline preserves something
    cx.set_state(indoc! {"
        > Item 1ˇ"
    });
    cx.update_editor(|editor, window, cx| {
        editor.newline(&Newline, window, cx);
    });
    cx.assert_editor_state(indoc! {"
        > Item 1
        ˇ"
    });
}

#[gpui::test]
async fn test_paste_url_from_mav_copy_creates_markdown_link_over_selected_text(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state(&format!(
        "Hello, editor.\nMav is great (see this link: )\n«{url}ˇ»"
    ));

    cx.update_editor(|editor, window, cx| {
        editor.copy(&Copy, window, cx);
    });

    cx.set_state(&format!(
        "Hello, «editorˇ».\nMav is «ˇgreat» (see this link: ˇ)\n{url}"
    ));

    cx.update_editor(|editor, window, cx| {
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "Hello, [editor]({url})ˇ.\nMav is [great]({url})ˇ (see this link: {url}ˇ)\n{url}"
    ));
}

#[gpui::test]
async fn test_paste_url_from_other_app_replaces_existing_url_without_creating_markdown_link(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("Please visit mav's homepage: «https://www.apple.comˇ»");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!("Please visit mav's homepage: {url}ˇ"));
}

#[gpui::test]
async fn test_paste_plain_text_from_other_app_replaces_selection_without_creating_markdown_link(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let text = "Awesome";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("Hello, «editorˇ».\nMav is «ˇgreat»");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!("Hello, {text}ˇ.\nMav is {text}ˇ"));
}

#[gpui::test]
async fn test_paste_text_with_scheme_like_prefix_replaces_selection_without_creating_markdown_link(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    // `url::Url::parse` accepts this as a URL with the scheme `editor`, but it
    // should not be treated as one when pasting.
    let text = "editor: Fix double-click bracket selection for large spans";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("«(feat on git-ui-add-info-exclude-to-context-menus) Fmtˇ»");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!("{text}ˇ"));
}

#[gpui::test]
async fn test_paste_url_from_other_app_without_creating_markdown_link_in_non_markdown_language(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(markdown_language), cx));
    cx.set_state("// Hello, «editorˇ».\n// Mav is «ˇgreat» (see this link: ˇ)");

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "// Hello, {url}ˇ.\n// Mav is {url}ˇ (see this link: {url}ˇ)"
    ));
}

#[gpui::test]
async fn test_paste_url_from_other_app_creates_markdown_link_selectively_in_multi_buffer(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let url = "https://mav.dev";

    let markdown_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Markdown".into(),
            ..LanguageConfig::default()
        },
        None,
    ));

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("this will embed -> link", vec![Point::row_range(0..1)]),
                ("this will replace -> link", vec![Point::row_range(0..1)]),
            ],
            cx,
        );
        let mut editor = Editor::new(EditorMode::full(), multi_buffer.clone(), None, window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(vec![
                Point::new(0, 19)..Point::new(0, 23),
                Point::new(1, 21)..Point::new(1, 25),
            ])
        });
        let snapshot = multi_buffer.read(cx).snapshot(cx);
        let first_buffer_id = snapshot.all_buffer_ids().next().unwrap();
        let first_buffer = multi_buffer.read(cx).buffer(first_buffer_id).unwrap();
        first_buffer.update(cx, |buffer, cx| {
            buffer.set_language(Some(markdown_language.clone()), cx);
        });

        editor
    });
    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;

    cx.update_editor(|editor, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(url.to_string()));
        editor.paste(&Paste, window, cx);
    });

    cx.assert_editor_state(&format!(
        "this will embed -> [link]({url})ˇ\nthis will replace -> {url}ˇ"
    ));
}

#[gpui::test]
async fn test_race_in_multibuffer_save(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            "first.rs": "# First Document\nSome content here.",
            "second.rs": "Plain text content for second file.",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let language = rust_lang();
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(language.clone());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            ..FakeLspAdapter::default()
        },
    );

    let buffer1 = project
        .update(cx, |project, cx| {
            project.open_local_buffer(PathBuf::from(path!("/project/first.rs")), cx)
        })
        .await
        .unwrap();
    let buffer2 = project
        .update(cx, |project, cx| {
            project.open_local_buffer(PathBuf::from(path!("/project/second.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(Capability::ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::for_buffer(&buffer1, cx),
            buffer1.clone(),
            [Point::zero()..buffer1.read(cx).max_point()],
            3,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::for_buffer(&buffer2, cx),
            buffer2.clone(),
            [Point::zero()..buffer1.read(cx).max_point()],
            3,
            cx,
        );
        multi_buffer
    });

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    let fake_language_server = fake_servers.next().await.unwrap();

    buffer1.update(cx, |buffer, cx| buffer.edit([(0..0, "hello!")], None, cx));

    let save = editor.update_in(cx, |editor, window, cx| {
        assert!(editor.is_dirty(cx));

        editor.save(
            SaveOptions {
                format: true,
                force_format: false,
                autosave: true,
            },
            project,
            window,
            cx,
        )
    });
    let (start_edit_tx, start_edit_rx) = oneshot::channel();
    let (done_edit_tx, done_edit_rx) = oneshot::channel();
    let mut done_edit_rx = Some(done_edit_rx);
    let mut start_edit_tx = Some(start_edit_tx);

    fake_language_server.set_request_handler::<lsp::request::Formatting, _, _>(move |_, _| {
        start_edit_tx.take().unwrap().send(()).unwrap();
        let done_edit_rx = done_edit_rx.take().unwrap();
        async move {
            done_edit_rx.await.unwrap();
            Ok(None)
        }
    });

    start_edit_rx.await.unwrap();
    buffer2
        .update(cx, |buffer, cx| buffer.edit([(0..0, "world!")], None, cx))
        .unwrap();

    done_edit_tx.send(()).unwrap();

    save.await.unwrap();
    cx.update(|_, cx| assert!(editor.is_dirty(cx)));
}

#[gpui::test]
fn test_duplicate_line_up_on_last_line_without_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("line1\nline2", cx);
        build_editor(buffer, window, cx)
    });

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)
                ])
            });

            editor.duplicate_line_up(&DuplicateLineUp, window, cx);

            assert_eq!(
                editor.display_text(cx),
                "line1\nline2\nline2",
                "Duplicating last line upward should create duplicate above, not on same line"
            );

            assert_eq!(
                editor
                    .selections
                    .display_ranges(&editor.display_snapshot(cx)),
                vec![DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)],
                "Selection should move to the duplicated line"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_copy_line_without_trailing_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("line1\nline2ˇ");

    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| item.text().as_deref().map(str::to_string));

    assert_eq!(
        clipboard_text,
        Some("line2\n".to_string()),
        "Copying a line without trailing newline should include a newline"
    );

    cx.set_state("line1\nˇ");

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));

    cx.assert_editor_state("line1\nline2\nˇ");
}

#[gpui::test]
async fn test_multi_selection_copy_with_newline_between_copied_lines(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("ˇline1\nˇline2\nˇline3\n");

    cx.update_editor(|e, window, cx| e.copy(&Copy, window, cx));

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| item.text().as_deref().map(str::to_string));

    assert_eq!(
        clipboard_text,
        Some("line1\nline2\nline3\n".to_string()),
        "Copying multiple lines should include a single newline between lines"
    );

    cx.set_state("lineA\nˇ");

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));

    cx.assert_editor_state("lineA\nline1\nline2\nline3\nˇ");
}

#[gpui::test]
async fn test_multi_selection_cut_with_newline_between_copied_lines(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("ˇline1\nˇline2\nˇline3\n");

    cx.update_editor(|e, window, cx| e.cut(&Cut, window, cx));

    let clipboard_text = cx
        .read_from_clipboard()
        .and_then(|item| item.text().as_deref().map(str::to_string));

    assert_eq!(
        clipboard_text,
        Some("line1\nline2\nline3\n".to_string()),
        "Copying multiple lines should include a single newline between lines"
    );

    cx.set_state("lineA\nˇ");

    cx.update_editor(|e, window, cx| e.paste(&Paste, window, cx));

    cx.assert_editor_state("lineA\nline1\nline2\nline3\nˇ");
}

#[gpui::test]
async fn test_end_of_editor_context(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("line1\nline2ˇ");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::SingleLine);
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("ˇline1\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::SingleLine);
        assert!(e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("line1ˇ\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::SingleLine);
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });

    cx.set_state("line1\nline2ˇ");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::AutoHeight {
            min_lines: 1,
            max_lines: Some(4),
        });
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("ˇline1\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::AutoHeight {
            min_lines: 1,
            max_lines: Some(4),
        });
        assert!(e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });
    cx.set_state("line1ˇ\nline2");
    cx.update_editor(|e, window, cx| {
        e.set_mode(EditorMode::AutoHeight {
            min_lines: 1,
            max_lines: Some(4),
        });
        assert!(!e.key_context(window, cx).contains("start_of_input"));
        assert!(!e.key_context(window, cx).contains("end_of_input"));
    });
}

#[gpui::test]
async fn test_sticky_scroll(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

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

    let fn_foo = Point { row: 0, column: 0 };
    let impl_bar = Point { row: 4, column: 0 };
    let fn_new = Point { row: 5, column: 4 };

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(0.5), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(1.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(1.5), vec![(fn_foo, -0.5)]);
    assert_eq!(sticky_headers(2.0), vec![]);
    assert_eq!(sticky_headers(2.5), vec![]);
    assert_eq!(sticky_headers(3.0), vec![]);
    assert_eq!(sticky_headers(3.5), vec![]);
    assert_eq!(sticky_headers(4.0), vec![]);
    assert_eq!(sticky_headers(4.5), vec![(impl_bar, 0.0), (fn_new, 1.0)]);
    assert_eq!(sticky_headers(5.0), vec![(impl_bar, 0.0), (fn_new, 1.0)]);
    assert_eq!(sticky_headers(5.5), vec![(impl_bar, 0.0), (fn_new, 0.5)]);
    assert_eq!(sticky_headers(6.0), vec![(impl_bar, 0.0)]);
    assert_eq!(sticky_headers(6.5), vec![(impl_bar, 0.0)]);
    assert_eq!(sticky_headers(7.0), vec![(impl_bar, 0.0)]);
    assert_eq!(sticky_headers(7.5), vec![(impl_bar, -0.5)]);
    assert_eq!(sticky_headers(8.0), vec![]);
    assert_eq!(sticky_headers(8.5), vec![]);
    assert_eq!(sticky_headers(9.0), vec![]);
    assert_eq!(sticky_headers(9.5), vec![]);
    assert_eq!(sticky_headers(10.0), vec![]);
}

#[gpui::test]
async fn test_sticky_scroll_with_decoration_prefix_in_item(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "TypeScript".into(),
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        )
        .with_outline_query(
            r#"
            (class_declaration
                "class" @context
                name: (_) @name) @item
            "#,
        )
        .expect("TypeScript outline query"),
    );

    let buffer = indoc! {"
        ˇ@Decorator
        class Foo {
            x = 1;
            y = 2;
            z = 3;
            w = 4;
        }
    "};
    cx.set_state(buffer);
    cx.update_editor(|e, _, cx| {
        e.buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(Some(language), cx);
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

    let class_foo = Point { row: 1, column: 0 };

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(1.5), vec![(class_foo, 0.0)]);
    assert_eq!(sticky_headers(2.5), vec![(class_foo, 0.0)]);
    assert_eq!(sticky_headers(5.5), vec![(class_foo, -0.5)]);
    assert_eq!(sticky_headers(6.0), vec![]);
    assert_eq!(sticky_headers(7.0), vec![]);
}

#[gpui::test]
async fn test_sticky_scroll_anchors_multiline_c_signature_on_name_row(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let buffer = indoc! {"
        ˇvoid
        evdev_post_scroll(struct evdev_device *device,
                  usec_t time,
                  enum libinput_pointer_axis_source source,
                  const struct normalized_coords *delta)
        {
            const struct normalized_coords tilt_rot = {
                cos(SCROLL_DELTA_TILT_ANGLE),
                sin(SCROLL_DELTA_TILT_ANGLE),
            };
        }
    "};
    cx.set_state(buffer);

    cx.update_editor(|editor, _, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
            .update(cx, |buffer, cx| {
                buffer.set_language(
                    Some(languages::language("c", tree_sitter_c::LANGUAGE.into())),
                    cx,
                );
            })
    });

    let mut sticky_headers = |offset: ScrollOffset| {
        cx.update_editor(|editor, window, cx| {
            editor.scroll(gpui::Point { x: 0., y: offset }, None, window, cx);
        });
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| {
            EditorElement::sticky_headers(&editor, &editor.snapshot(window, cx))
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

    let function_name_row = Point { row: 1, column: 0 };

    assert_eq!(sticky_headers(1.0), vec![]);
    assert_eq!(sticky_headers(1.5), vec![(function_name_row, 0.0)]);
    assert_eq!(sticky_headers(5.0), vec![(function_name_row, 0.0)]);
}

#[gpui::test]
async fn test_sticky_scroll_with_expanded_deleted_diff_hunks(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = indoc! {"
        fn foo() {
            let a = 1;
            let b = 2;
            let c = 3;
            let d = 4;
            let e = 5;
        }
    "};

    let buffer = indoc! {"
        ˇfn foo() {
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

    cx.set_head_text(diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    // After expanding, the display should look like:
    //   row 0: fn foo() {
    //   row 1: -    let a = 1;   (deleted)
    //   row 2: -    let b = 2;   (deleted)
    //   row 3: -    let c = 3;   (deleted)
    //   row 4: -    let d = 4;   (deleted)
    //   row 5: -    let e = 5;   (deleted)
    //   row 6: }
    //
    // fn foo() spans display rows 0-6. Scrolling into the deleted region
    // (rows 1-5) should still show fn foo() as a sticky header.

    let fn_foo = Point { row: 0, column: 0 };

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

    assert_eq!(sticky_headers(0.0), vec![]);
    assert_eq!(sticky_headers(0.5), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(1.0), vec![(fn_foo, 0.0)]);
    // Scrolling into deleted lines: fn foo() should still be a sticky header.
    assert_eq!(sticky_headers(2.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(3.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(4.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(5.0), vec![(fn_foo, 0.0)]);
    assert_eq!(sticky_headers(5.5), vec![(fn_foo, -0.5)]);
    // Past the closing brace: no more sticky header.
    assert_eq!(sticky_headers(6.0), vec![]);
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

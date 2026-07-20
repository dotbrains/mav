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
#[path = "editor_tests/clipboard_selection.rs"]
mod clipboard_selection;
#[path = "editor_tests/completion_commands.rs"]
mod completion_commands;
#[path = "editor_tests/completion_core.rs"]
mod completion_core;
#[path = "editor_tests/completion_modes.rs"]
mod completion_modes;
#[path = "editor_tests/completion_replace.rs"]
mod completion_replace;
#[path = "editor_tests/cursor_line_word.rs"]
mod cursor_line_word;
#[path = "editor_tests/cursor_movement_basic.rs"]
mod cursor_movement_basic;
#[path = "editor_tests/delete_boundaries.rs"]
mod delete_boundaries;
#[path = "editor_tests/delete_brackets_words.rs"]
mod delete_brackets_words;
#[path = "editor_tests/events_input.rs"]
mod events_input;
#[path = "editor_tests/folding_basic.rs"]
mod folding_basic;
#[path = "editor_tests/folding_multiline.rs"]
mod folding_multiline;
#[path = "editor_tests/format_on_save.rs"]
mod format_on_save;
#[path = "editor_tests/format_requests.rs"]
mod format_requests;
#[path = "editor_tests/formatter_selection.rs"]
mod formatter_selection;
#[path = "editor_tests/indent_outdent.rs"]
mod indent_outdent;
#[path = "editor_tests/join_lines_comments.rs"]
mod join_lines_comments;
#[path = "editor_tests/line_operations.rs"]
mod line_operations;
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
#[path = "editor_tests/range_formatting.rs"]
mod range_formatting;
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

#[gpui::test]
async fn test_completion_page_up_down_keys(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
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
                    label: "first".into(),
                    ..Default::default()
                },
                lsp::CompletionItem {
                    label: "last".into(),
                    ..Default::default()
                },
            ])))
        });
    cx.set_state("variableˇ");
    cx.simulate_keystroke(".");
    cx.executor().run_until_parked();

    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["first", "last"]);
        } else {
            panic!("expected completion menu to be open");
        }
    });

    cx.update_editor(|editor, window, cx| {
        editor.move_page_down(&MovePageDown::default(), window, cx);
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert!(
                menu.selected_item == 1,
                "expected PageDown to select the last item from the context menu"
            );
        } else {
            panic!("expected completion menu to stay open after PageDown");
        }
    });

    cx.update_editor(|editor, window, cx| {
        editor.move_page_up(&MovePageUp::default(), window, cx);
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert!(
                menu.selected_item == 0,
                "expected PageUp to select the first item from the context menu"
            );
        } else {
            panic!("expected completion menu to stay open after PageUp");
        }
    });
}

#[gpui::test]
async fn test_as_is_completions(cx: &mut TestAppContext) {
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
                                line: 1,
                                character: 2,
                            },
                            end: lsp::Position {
                                line: 1,
                                character: 3,
                            },
                        },
                        new_text: "unsafe".to_string(),
                    })),
                    insert_text_mode: Some(lsp::InsertTextMode::AS_IS),
                    ..Default::default()
                },
            ])))
        });
    cx.set_state("fn a() {}\n  nˇ");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.trigger_completion_on_input("n", true, window, cx)
    });
    cx.executor().run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.confirm_completion(&Default::default(), window, cx)
    });
    cx.executor().run_until_parked();
    cx.assert_editor_state("fn a() {}\n  unsafeˇ");
}

#[gpui::test]
async fn test_panic_during_c_completions(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let language =
        Arc::try_unwrap(languages::language("c", tree_sitter_c::LANGUAGE.into())).unwrap();
    let mut cx = EditorLspTestContext::new(
        language,
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                ..lsp::CompletionOptions::default()
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    cx.set_state(
        "#ifndef BAR_H
#define BAR_H

#include <stdbool.h>

int fn_branch(bool do_branch1, bool do_branch2);

#endif // BAR_H
ˇ",
    );
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("#", window, cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("i", window, cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("n", window, cx);
    });
    cx.executor().run_until_parked();
    cx.assert_editor_state(
        "#ifndef BAR_H
#define BAR_H

#include <stdbool.h>

int fn_branch(bool do_branch1, bool do_branch2);

#endif // BAR_H
#inˇ",
    );

    cx.lsp
        .set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                is_incomplete: false,
                item_defaults: None,
                items: vec![lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::SNIPPET),
                    label_details: Some(lsp::CompletionItemLabelDetails {
                        detail: Some("header".to_string()),
                        description: None,
                    }),
                    label: " include".to_string(),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range {
                            start: lsp::Position {
                                line: 8,
                                character: 1,
                            },
                            end: lsp::Position {
                                line: 8,
                                character: 1,
                            },
                        },
                        new_text: "include \"$0\"".to_string(),
                    })),
                    sort_text: Some("40b67681include".to_string()),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    filter_text: Some("include".to_string()),
                    insert_text: Some("include \"$0\"".to_string()),
                    ..lsp::CompletionItem::default()
                }],
            })))
        });
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.confirm_completion(&ConfirmCompletion::default(), window, cx)
    });
    cx.executor().run_until_parked();
    cx.assert_editor_state(
        "#ifndef BAR_H
#define BAR_H

#include <stdbool.h>

int fn_branch(bool do_branch1, bool do_branch2);

#endif // BAR_H
#include \"ˇ\"",
    );

    cx.lsp
        .set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                is_incomplete: true,
                item_defaults: None,
                items: vec![lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FILE),
                    label: "AGL/".to_string(),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range {
                            start: lsp::Position {
                                line: 8,
                                character: 10,
                            },
                            end: lsp::Position {
                                line: 8,
                                character: 11,
                            },
                        },
                        new_text: "AGL/".to_string(),
                    })),
                    sort_text: Some("40b67681AGL/".to_string()),
                    insert_text_format: Some(lsp::InsertTextFormat::PLAIN_TEXT),
                    filter_text: Some("AGL/".to_string()),
                    insert_text: Some("AGL/".to_string()),
                    ..lsp::CompletionItem::default()
                }],
            })))
        });
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.confirm_completion(&ConfirmCompletion::default(), window, cx)
    });
    cx.executor().run_until_parked();
    cx.assert_editor_state(
        r##"#ifndef BAR_H
#define BAR_H

#include <stdbool.h>

int fn_branch(bool do_branch1, bool do_branch2);

#endif // BAR_H
#include "AGL/ˇ"##,
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\"", window, cx);
    });
    cx.executor().run_until_parked();
    cx.assert_editor_state(
        r##"#ifndef BAR_H
#define BAR_H

#include <stdbool.h>

int fn_branch(bool do_branch1, bool do_branch2);

#endif // BAR_H
#include "AGL/"ˇ"##,
    );
}

#[gpui::test]
async fn test_no_duplicated_completion_requests(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(false),
                ..lsp::CompletionOptions::default()
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");
    let completion_item = lsp::CompletionItem {
        label: "Some".into(),
        kind: Some(lsp::CompletionItemKind::SNIPPET),
        detail: Some("Wrap the expression in an `Option::Some`".to_string()),
        documentation: Some(lsp::Documentation::MarkupContent(lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: "```rust\nSome(2)\n```".to_string(),
        })),
        deprecated: Some(false),
        sort_text: Some("Some".to_string()),
        filter_text: Some("Some".to_string()),
        insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 22,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "Some(2)".to_string(),
        })),
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 20,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "".to_string(),
        }]),
        ..Default::default()
    };

    let closure_completion_item = completion_item.clone();
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    let mut request = cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let task_completion_item = closure_completion_item.clone();
        counter_clone.fetch_add(1, atomic::Ordering::Release);
        async move {
            Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                is_incomplete: true,
                item_defaults: None,
                items: vec![task_completion_item],
            })))
        }
    });

    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.assert_editor_state("fn main() { let a = 2.ˇ; }");
    assert!(request.next().await.is_some());
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);

    cx.simulate_keystrokes("S o m");
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.assert_editor_state("fn main() { let a = 2.Somˇ; }");
    assert!(request.next().await.is_some());
    assert!(request.next().await.is_some());
    assert!(request.next().await.is_some());
    request.close();
    assert!(request.next().await.is_none());
    assert_eq!(
        counter.load(atomic::Ordering::Acquire),
        4,
        "With the completions menu open, only one LSP request should happen per input"
    );
}

#[gpui::test]
async fn test_toggle_comment(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into(), "//! ".into(), "/// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // If multiple selections intersect a line, the line is only toggled once.
    cx.set_state(indoc! {"
        fn a() {
            «//b();
            ˇ»// «c();
            //ˇ»  d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «b();
            ˇ»«c();
            ˇ» d();
        }
    "});

    // The comment prefix is inserted at the same column for every line in a
    // selection.
    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            // «b();
            ˇ»// «c();
            ˇ» // d();
        }
    "});

    // If a selection ends at the beginning of a line, that line is not toggled.
    cx.set_selections_state(indoc! {"
        fn a() {
            // b();
            «// c();
        ˇ»     // d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            // b();
            «c();
        ˇ»     // d();
        }
    "});

    // If a selection span a single line and is empty, the line is toggled.
    cx.set_state(indoc! {"
        fn a() {
            a();
            b();
        ˇ
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            a();
            b();
        //•ˇ
        }
    "});

    // If a selection span multiple lines, empty lines are not toggled.
    cx.set_state(indoc! {"
        fn a() {
            «a();

            c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            // «a();

            // c();ˇ»
        }
    "});

    // If a selection includes multiple comment prefixes, all lines are uncommented.
    cx.set_state(indoc! {"
        fn a() {
            «// a();
            /// b();
            //! c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(&ToggleComments::default(), window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «a();
            b();
            c();ˇ»
        }
    "});
}

#[gpui::test]
async fn test_toggle_comment_ignore_indent(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into(), "//! ".into(), "/// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    let toggle_comments = &ToggleComments {
        advance_downwards: false,
        ignore_indent: true,
    };

    // If multiple selections intersect a line, the line is only toggled once.
    cx.set_state(indoc! {"
        fn a() {
        //    «b();
        //    c();
        //    ˇ» d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «b();
            c();
            ˇ» d();
        }
    "});

    // The comment prefix is inserted at the beginning of each line
    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
        //    «b();
        //    c();
        //    ˇ» d();
        }
    "});

    // If a selection ends at the beginning of a line, that line is not toggled.
    cx.set_selections_state(indoc! {"
        fn a() {
        //    b();
        //    «c();
        ˇ»//     d();
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
        //    b();
            «c();
        ˇ»//     d();
        }
    "});

    // If a selection span a single line and is empty, the line is toggled.
    cx.set_state(indoc! {"
        fn a() {
            a();
            b();
        ˇ
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            a();
            b();
        //ˇ
        }
    "});

    // If a selection span multiple lines, empty lines are not toggled.
    cx.set_state(indoc! {"
        fn a() {
            «a();

            c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
        //    «a();

        //    c();ˇ»
        }
    "});

    // If a selection includes multiple comment prefixes, all lines are uncommented.
    cx.set_state(indoc! {"
        fn a() {
        //    «a();
        ///    b();
        //!    c();ˇ»
        }
    "});

    cx.update_editor(|e, window, cx| e.toggle_comments(toggle_comments, window, cx));

    cx.assert_editor_state(indoc! {"
        fn a() {
            «a();
            b();
            c();ˇ»
        }
    "});
}

#[gpui::test]
async fn test_advance_downward_on_toggle_comment(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig {
            line_comments: vec!["// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let mut cx = EditorTestContext::new(cx).await;

    cx.language_registry().add(language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    let toggle_comments = &ToggleComments {
        advance_downwards: true,
        ignore_indent: false,
    };

    // Single cursor on one line -> advance
    // Cursor moves horizontally 3 characters as well on non-blank line
    cx.set_state(indoc!(
        "fn a() {
             ˇdog();
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
             catˇ();
        }"
    ));

    // Single selection on one line -> don't advance
    cx.set_state(indoc!(
        "fn a() {
             «dog()ˇ»;
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // «dog()ˇ»;
             cat();
        }"
    ));

    // Multiple cursors on one line -> advance
    cx.set_state(indoc!(
        "fn a() {
             ˇdˇog();
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
             catˇ(ˇ);
        }"
    ));

    // Multiple cursors on one line, with selection -> don't advance
    cx.set_state(indoc!(
        "fn a() {
             ˇdˇog«()ˇ»;
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // ˇdˇog«()ˇ»;
             cat();
        }"
    ));

    // Single cursor on one line -> advance
    // Cursor moves to column 0 on blank line
    cx.set_state(indoc!(
        "fn a() {
             ˇdog();

             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
        ˇ
             cat();
        }"
    ));

    // Single cursor on one line -> advance
    // Cursor starts and ends at column 0
    cx.set_state(indoc!(
        "fn a() {
         ˇ    dog();
             cat();
        }"
    ));
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(toggle_comments, window, cx);
    });
    cx.assert_editor_state(indoc!(
        "fn a() {
             // dog();
         ˇ    cat();
        }"
    ));
}

#[gpui::test]
async fn test_toggle_block_comment(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let html_language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "HTML".into(),
                block_comment: Some(BlockCommentConfig {
                    start: "<!-- ".into(),
                    prefix: "".into(),
                    end: " -->".into(),
                    tab_size: 0,
                }),
                ..Default::default()
            },
            Some(tree_sitter_html::LANGUAGE.into()),
        )
        .with_injection_query(
            r#"
            (script_element
                (raw_text) @injection.content
                (#set! injection.language "javascript"))
            "#,
        )
        .unwrap(),
    );

    let javascript_language = Arc::new(Language::new(
        LanguageConfig {
            name: "JavaScript".into(),
            line_comments: vec!["// ".into()],
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    ));

    cx.language_registry().add(html_language.clone());
    cx.language_registry().add(javascript_language);
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(html_language), cx);
    });

    // Toggle comments for empty selections
    cx.set_state(
        &r#"
            <p>A</p>ˇ
            <p>B</p>ˇ
            <p>C</p>ˇ
        "#
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <!-- <p>A</p>ˇ -->
            <!-- <p>B</p>ˇ -->
            <!-- <p>C</p>ˇ -->
        "#
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <p>A</p>ˇ
            <p>B</p>ˇ
            <p>C</p>ˇ
        "#
        .unindent(),
    );

    // Toggle comments for mixture of empty and non-empty selections, where
    // multiple selections occupy a given line.
    cx.set_state(
        &r#"
            <p>A«</p>
            <p>ˇ»B</p>ˇ
            <p>C«</p>
            <p>ˇ»D</p>ˇ
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <!-- <p>A«</p>
            <p>ˇ»B</p>ˇ -->
            <!-- <p>C«</p>
            <p>ˇ»D</p>ˇ -->
        "#
        .unindent(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    cx.assert_editor_state(
        &r#"
            <p>A«</p>
            <p>ˇ»B</p>ˇ
            <p>C«</p>
            <p>ˇ»D</p>ˇ
        "#
        .unindent(),
    );

    // Toggle comments when different languages are active for different
    // selections.
    cx.set_state(
        &r#"
            ˇ<script>
                ˇvar x = new Y();
            ˇ</script>
        "#
        .unindent(),
    );
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_comments(&ToggleComments::default(), window, cx)
    });
    // TODO this is how it actually worked in Mav Stable, which is not very ergonomic.
    // Uncommenting and commenting from this position brings in even more wrong artifacts.
    cx.assert_editor_state(
        &r#"
            <!-- ˇ<script> -->
                // ˇvar x = new Y();
            <!-- ˇ</script> -->
        "#
        .unindent(),
    );
}

#[gpui::test]
fn test_editing_disjoint_excerpts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer = cx.new(|cx| Buffer::local(sample_text(6, 4, 'a'), cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::new(0, 0)..Point::new(0, 4),
                Point::new(5, 0)..Point::new(5, 4),
            ],
            0,
            cx,
        );
        assert_eq!(multibuffer.read(cx).text(), "aaaa\nffff");
        multibuffer
    });

    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(multibuffer, window, cx));
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "aaaa\nffff");
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                Point::new(0, 0)..Point::new(0, 0),
                Point::new(1, 0)..Point::new(1, 0),
            ])
        });

        editor.handle_input("X", window, cx);
        assert_eq!(editor.text(cx), "Xaaaa\nXffff");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [
                Point::new(0, 1)..Point::new(0, 1),
                Point::new(1, 1)..Point::new(1, 1),
            ]
        );

        // Ensure the cursor's head is respected when deleting across an excerpt boundary.
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 2)..Point::new(1, 2)])
        });
        editor.backspace(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "Xa\nfff");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [Point::new(1, 0)..Point::new(1, 0)]
        );

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 1)..Point::new(0, 1)])
        });
        editor.backspace(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "X\nff");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            [Point::new(0, 1)..Point::new(0, 1)]
        );
    });
}

#[gpui::test]
fn test_header_jump_data_uses_selection_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // 25-line buffer so excerpts at rows 1, 10, and 20 (each a 1-line range,
    // expanded by 2 context lines) can't merge into a single excerpt.
    let buffer_text = (0..25)
        .map(|row| format!("line {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = cx.new(|cx| Buffer::local(buffer_text, cx));
    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::new(1, 0)..Point::new(1, 0),
                Point::new(10, 0)..Point::new(10, 0),
                Point::new(20, 0)..Point::new(20, 0),
            ],
            2,
            cx,
        );
        multibuffer
    });

    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(multibuffer, window, cx));

    editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let display_snapshot = editor.display_snapshot(cx);

        // Ensure the three ranges landed in three separate excerpts.
        let excerpts: Vec<_> = snapshot
            .buffer_snapshot()
            .excerpts_for_buffer(buffer_id)
            .collect();
        assert_eq!(excerpts.len(), 3);

        // Place the cursor at the start of the third excerpt, expressed in
        // terms of the underlying buffer.
        let selection_buffer_row = 20;
        let buffer_entity = editor.buffer().read(cx).buffer(buffer_id).unwrap();
        let selection_anchor = editor.buffer().update(cx, |multibuffer, cx| {
            multibuffer
                .buffer_point_to_anchor(&buffer_entity, Point::new(selection_buffer_row, 0), cx)
                .expect("buffer row 20 maps to a multibuffer anchor")
        });
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_anchor_ranges([selection_anchor..selection_anchor])
        });

        let mut latest_selection_anchors: HashMap<BufferId, Anchor> = HashMap::default();
        for selection in editor.selections.all_anchors(&display_snapshot).iter() {
            let head = selection.head();
            if let Some((text_anchor, _)) = snapshot.buffer_snapshot().anchor_to_buffer_anchor(head)
            {
                latest_selection_anchors.insert(text_anchor.buffer_id, head);
            }
        }

        // The sticky buffer header represents the FIRST excerpt of its buffer,
        // even when the cursor is in a later excerpt. That mismatch is the
        // precondition for the regression.
        let first_excerpt = snapshot
            .buffer_snapshot()
            .excerpt_boundaries_in_range(MultiBufferOffset(0)..snapshot.buffer_snapshot().len())
            .next()
            .expect("multibuffer has at least one excerpt")
            .next;

        let jump_data = header_jump_data(
            &snapshot,
            DisplayRow(0),
            FILE_HEADER_HEIGHT + MULTI_BUFFER_EXCERPT_HEADER_HEIGHT,
            &first_excerpt,
            &latest_selection_anchors,
        );

        match jump_data {
            JumpData::MultiBufferPoint {
                position,
                line_offset_from_top,
                ..
            } => {
                assert_eq!(
                    position.row, selection_buffer_row,
                    "jump should target the cursor's buffer row, not the first excerpt's row"
                );
                assert!(
                    line_offset_from_top < selection_buffer_row,
                    "line_offset_from_top ({line_offset_from_top}) should be measured from the \
                     selection's excerpt, not the first excerpt; expected less than \
                     selection_buffer_row ({selection_buffer_row})"
                );
            }
            JumpData::MultiBufferRow { .. } => {
                panic!("expected MultiBufferPoint jump data when a selection is present")
            }
        }
    });
}

#[gpui::test]
async fn test_extra_newline_insertion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "{".to_string(),
                            end: "}".to_string(),
                            close: true,
                            surround: true,
                            newline: true,
                        },
                        BracketPair {
                            start: "/* ".to_string(),
                            end: " */".to_string(),
                            close: true,
                            surround: true,
                            newline: true,
                        },
                    ],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query("")
        .unwrap(),
    );

    let text = concat!(
        "{   }\n",     //
        "  x\n",       //
        "  /*   */\n", //
        "x\n",         //
        "{{} }\n",     //
    );

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 3),
                DisplayPoint::new(DisplayRow(2), 5)..DisplayPoint::new(DisplayRow(2), 5),
                DisplayPoint::new(DisplayRow(4), 4)..DisplayPoint::new(DisplayRow(4), 4),
            ])
        });
        editor.newline(&Newline, window, cx);

        assert_eq!(
            editor.buffer().read(cx).read(cx).text(),
            concat!(
                "{ \n",    // Suppress rustfmt
                "\n",      //
                "}\n",     //
                "  x\n",   //
                "  /* \n", //
                "  \n",    //
                "  */\n",  //
                "x\n",     //
                "{{} \n",  //
                "}\n",     //
            )
        );
    });
}

#[gpui::test]
fn test_highlighted_ranges(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(16, 8, 'a'), cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        let buffer = editor.buffer.read(cx).snapshot(cx);

        let anchor_range =
            |range: Range<Point>| buffer.anchor_after(range.start)..buffer.anchor_after(range.end);

        editor.highlight_background(
            HighlightKey::ColorizeBracket(0),
            &[
                anchor_range(Point::new(2, 1)..Point::new(2, 3)),
                anchor_range(Point::new(4, 2)..Point::new(4, 4)),
                anchor_range(Point::new(6, 3)..Point::new(6, 5)),
                anchor_range(Point::new(8, 4)..Point::new(8, 6)),
            ],
            |_, _| Hsla::red(),
            cx,
        );
        editor.highlight_background(
            HighlightKey::ColorizeBracket(1),
            &[
                anchor_range(Point::new(3, 2)..Point::new(3, 5)),
                anchor_range(Point::new(5, 3)..Point::new(5, 6)),
                anchor_range(Point::new(7, 4)..Point::new(7, 7)),
                anchor_range(Point::new(9, 5)..Point::new(9, 8)),
            ],
            |_, _| Hsla::green(),
            cx,
        );

        let snapshot = editor.snapshot(window, cx);
        let highlighted_ranges = editor.sorted_background_highlights_in_range(
            anchor_range(Point::new(3, 4)..Point::new(7, 4)),
            &snapshot,
            cx.theme(),
        );
        assert_eq!(
            highlighted_ranges,
            &[
                (
                    DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(3), 5),
                    Hsla::green(),
                ),
                (
                    DisplayPoint::new(DisplayRow(4), 2)..DisplayPoint::new(DisplayRow(4), 4),
                    Hsla::red(),
                ),
                (
                    DisplayPoint::new(DisplayRow(5), 3)..DisplayPoint::new(DisplayRow(5), 6),
                    Hsla::green(),
                ),
                (
                    DisplayPoint::new(DisplayRow(6), 3)..DisplayPoint::new(DisplayRow(6), 5),
                    Hsla::red(),
                ),
            ]
        );
        assert_eq!(
            editor.sorted_background_highlights_in_range(
                anchor_range(Point::new(5, 6)..Point::new(6, 4)),
                &snapshot,
                cx.theme(),
            ),
            &[(
                DisplayPoint::new(DisplayRow(6), 3)..DisplayPoint::new(DisplayRow(6), 5),
                Hsla::red(),
            )]
        );
    });
}

#[gpui::test]
async fn test_copy_highlight_json(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        fn main() {
            let x = 1;ˇ
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "fn", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "main", "highlight": "function"},
                {"text": "()", "highlight": "punctuation.bracket"},
                {"text": " ", "highlight": null},
                {"text": "{", "highlight": "punctuation.bracket"},
            ],
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
            [
                {"text": "}", "highlight": "punctuation.bracket"},
            ],
        ])
    );
}

#[gpui::test]
async fn test_copy_highlight_json_selected_range(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        fn main() {
            «let x = 1;
            let yˇ» = 2;
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "y", "highlight": "variable"},
            ],
        ])
    );
}

#[gpui::test]
async fn test_copy_highlight_json_selected_line_range(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        fn main() {
            «let x = 1;
            let yˇ» = 2;
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.selections.set_line_mode(true);
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "y", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "2", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ],
        ])
    );
}

#[gpui::test]
async fn test_copy_highlight_json_single_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        fn main() {
            let ˇx = 1;
            let y = 2;
        }
    "});
    setup_syntax_highlighting(rust_lang(), &mut cx);

    cx.update_editor(|editor, window, cx| {
        editor.selections.set_line_mode(true);
        editor.copy_highlight_json(&CopyHighlightJson, window, cx);
    });

    let clipboard_json: serde_json::Value =
        serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap();
    assert_eq!(
        clipboard_json,
        json!([
            [
                {"text": "    ", "highlight": null},
                {"text": "let", "highlight": "keyword"},
                {"text": " ", "highlight": null},
                {"text": "x", "highlight": "variable"},
                {"text": " ", "highlight": null},
                {"text": "=", "highlight": "operator"},
                {"text": " ", "highlight": null},
                {"text": "1", "highlight": "number"},
                {"text": ";", "highlight": "punctuation.delimiter"},
            ]
        ])
    );
}

#[gpui::test]
async fn test_following(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, ["/file.rs".as_ref()], cx).await;

    let buffer = project.update(cx, |project, cx| {
        let buffer = project.create_local_buffer(&sample_text(16, 8, 'a'), None, false, cx);
        cx.new(|cx| MultiBuffer::singleton(buffer, cx))
    });
    let leader = cx.add_window(|window, cx| build_editor(buffer.clone(), window, cx));
    let follower = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::from_corners(
                    gpui::Point::new(px(0.), px(0.)),
                    gpui::Point::new(px(10.), px(80.)),
                ))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| build_editor(buffer.clone(), window, cx)),
        )
        .unwrap()
    });

    let is_still_following = Rc::new(RefCell::new(true));
    let follower_edit_event_count = Rc::new(RefCell::new(0));
    let pending_update = Rc::new(RefCell::new(None));
    let leader_entity = leader.root(cx).unwrap();
    let follower_entity = follower.root(cx).unwrap();
    _ = follower.update(cx, {
        let update = pending_update.clone();
        let is_still_following = is_still_following.clone();
        let follower_edit_event_count = follower_edit_event_count.clone();
        |_, window, cx| {
            cx.subscribe_in(
                &leader_entity,
                window,
                move |_, leader, event, window, cx| {
                    leader.update(cx, |leader, cx| {
                        leader.add_event_to_update_proto(
                            event,
                            &mut update.borrow_mut(),
                            window,
                            cx,
                        );
                    });
                },
            )
            .detach();

            cx.subscribe_in(
                &follower_entity,
                window,
                move |_, _, event: &EditorEvent, _window, _cx| {
                    if matches!(Editor::to_follow_event(event), Some(FollowEvent::Unfollow)) {
                        *is_still_following.borrow_mut() = false;
                    }

                    if let EditorEvent::BufferEdited = event {
                        *follower_edit_event_count.borrow_mut() += 1;
                    }
                },
            )
            .detach();
        }
    });

    // Update the selections only
    _ = leader.update(cx, |leader, window, cx| {
        leader.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(1)..MultiBufferOffset(1)])
        });
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![MultiBufferOffset(1)..MultiBufferOffset(1)]
        );
    });
    assert!(*is_still_following.borrow());
    assert_eq!(*follower_edit_event_count.borrow(), 0);

    // Update the scroll position only
    _ = leader.update(cx, |leader, window, cx| {
        leader.set_scroll_position(gpui::Point::new(1.5, 3.5), window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        follower
            .update(cx, |follower, _, cx| follower.scroll_position(cx))
            .unwrap(),
        gpui::Point::new(1.5, 3.5)
    );
    assert!(*is_still_following.borrow());
    assert_eq!(*follower_edit_event_count.borrow(), 0);

    // Update the selections and scroll position. The follower's scroll position is updated
    // via autoscroll, not via the leader's exact scroll position.
    _ = leader.update(cx, |leader, window, cx| {
        leader.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(0)])
        });
        leader.request_autoscroll(Autoscroll::newest(), cx);
        leader.set_scroll_position(gpui::Point::new(1.5, 3.5), window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(follower.scroll_position(cx), gpui::Point::new(1.5, 0.0));
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![MultiBufferOffset(0)..MultiBufferOffset(0)]
        );
    });
    assert!(*is_still_following.borrow());

    // Creating a pending selection that precedes another selection
    _ = leader.update(cx, |leader, window, cx| {
        leader.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(1)..MultiBufferOffset(1)])
        });
        leader.begin_selection(DisplayPoint::new(DisplayRow(0), 0), true, 1, window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![
                MultiBufferOffset(0)..MultiBufferOffset(0),
                MultiBufferOffset(1)..MultiBufferOffset(1)
            ]
        );
    });
    assert!(*is_still_following.borrow());

    // Extend the pending selection so that it surrounds another selection
    _ = leader.update(cx, |leader, window, cx| {
        leader.extend_selection(DisplayPoint::new(DisplayRow(0), 2), 1, window, cx);
    });
    follower
        .update(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                pending_update.borrow_mut().take().unwrap(),
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .unwrap();
    _ = follower.update(cx, |follower, _, cx| {
        assert_eq!(
            follower.selections.ranges(&follower.display_snapshot(cx)),
            vec![MultiBufferOffset(0)..MultiBufferOffset(2)]
        );
    });

    // Scrolling locally breaks the follow
    _ = follower.update(cx, |follower, window, cx| {
        let top_anchor = follower
            .buffer()
            .read(cx)
            .read(cx)
            .anchor_after(MultiBufferOffset(0));
        follower.set_scroll_anchor(
            ScrollAnchor {
                anchor: top_anchor,
                offset: gpui::Point::new(0.0, 0.5),
            },
            window,
            cx,
        );
    });
    assert!(!(*is_still_following.borrow()));
}

#[gpui::test]
async fn test_following_with_multiple_excerpts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, ["/file.rs".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let cx = &mut VisualTestContext::from_window(*window, cx);

    let leader = pane.update_in(cx, |_, window, cx| {
        let multibuffer = cx.new(|_| MultiBuffer::new(ReadWrite));
        cx.new(|cx| build_editor(multibuffer.clone(), window, cx))
    });

    // Start following the editor when it has no excerpts.
    let mut state_message =
        leader.update_in(cx, |leader, window, cx| leader.to_state_proto(window, cx));
    let workspace_entity = workspace.clone();
    let follower_1 = cx
        .update_window(*window, |_, window, cx| {
            Editor::from_state_proto(
                workspace_entity,
                ViewId {
                    creator: CollaboratorId::PeerId(PeerId::default()),
                    id: 0,
                },
                &mut state_message,
                window,
                cx,
            )
        })
        .unwrap()
        .unwrap()
        .await
        .unwrap();

    let update_message = Rc::new(RefCell::new(None));
    follower_1.update_in(cx, {
        let update = update_message.clone();
        |_, window, cx| {
            cx.subscribe_in(&leader, window, move |_, leader, event, window, cx| {
                leader.update(cx, |leader, cx| {
                    leader.add_event_to_update_proto(event, &mut update.borrow_mut(), window, cx);
                });
            })
            .detach();
        }
    });

    let (buffer_1, buffer_2) = project.update(cx, |project, cx| {
        (
            project.create_local_buffer("abc\ndef\nghi\njkl\nmno\npqr\nstu\nvwx\nyza\nbcd\nefg\nhij\nklm\nnop\nqrs\ntuv\nwxy\nzab\ncde\nfgh\n", None, false, cx),
            project.create_local_buffer("aaa\nbbb\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj\nkkk\nlll\nmmm\nnnn\nooo\nppp\nqqq\nrrr\nsss\nttt\n", None, false, cx),
        )
    });

    // Insert some excerpts.
    leader.update(cx, |leader, cx| {
        leader.buffer.update(cx, |multibuffer, cx| {
            multibuffer.set_excerpts_for_path(
                PathKey::with_sort_prefix(1, rel_path("b.txt").into_arc()),
                buffer_1.clone(),
                vec![
                    Point::row_range(0..3),
                    Point::row_range(1..6),
                    Point::row_range(12..15),
                ],
                0,
                cx,
            );
            multibuffer.set_excerpts_for_path(
                PathKey::with_sort_prefix(1, rel_path("a.txt").into_arc()),
                buffer_2.clone(),
                vec![Point::row_range(0..6), Point::row_range(8..12)],
                0,
                cx,
            );
        });
    });

    // Apply the update of adding the excerpts.
    follower_1
        .update_in(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                update_message.borrow().clone().unwrap(),
                window,
                cx,
            )
        })
        .await
        .unwrap();
    assert_eq!(
        follower_1.update(cx, |editor, cx| editor.text(cx)),
        leader.update(cx, |editor, cx| editor.text(cx))
    );
    update_message.borrow_mut().take();

    // Start following separately after it already has excerpts.
    let mut state_message =
        leader.update_in(cx, |leader, window, cx| leader.to_state_proto(window, cx));
    let workspace_entity = workspace.clone();
    let follower_2 = cx
        .update_window(*window, |_, window, cx| {
            Editor::from_state_proto(
                workspace_entity,
                ViewId {
                    creator: CollaboratorId::PeerId(PeerId::default()),
                    id: 0,
                },
                &mut state_message,
                window,
                cx,
            )
        })
        .unwrap()
        .unwrap()
        .await
        .unwrap();
    assert_eq!(
        follower_2.update(cx, |editor, cx| editor.text(cx)),
        leader.update(cx, |editor, cx| editor.text(cx))
    );

    // Remove some excerpts.
    leader.update(cx, |leader, cx| {
        leader.buffer.update(cx, |multibuffer, cx| {
            multibuffer.remove_excerpts(
                PathKey::with_sort_prefix(1, rel_path("b.txt").into_arc()),
                cx,
            );
        });
    });

    // Apply the update of removing the excerpts.
    follower_1
        .update_in(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                update_message.borrow().clone().unwrap(),
                window,
                cx,
            )
        })
        .await
        .unwrap();
    follower_2
        .update_in(cx, |follower, window, cx| {
            follower.apply_update_proto(
                &project,
                update_message.borrow().clone().unwrap(),
                window,
                cx,
            )
        })
        .await
        .unwrap();
    update_message.borrow_mut().take();
    assert_eq!(
        follower_1.update(cx, |editor, cx| editor.text(cx)),
        leader.update(cx, |editor, cx| editor.text(cx))
    );
}

#[gpui::test]
async fn go_to_prev_overlapping_diagnostic(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    cx.set_state(indoc! {"
        ˇfn func(abc def: i32) -> u32 {
        }
    "});

    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostics(
                    LanguageServerId(0),
                    lsp::PublishDiagnosticsParams {
                        uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                        version: None,
                        diagnostics: vec![
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 11),
                                    lsp::Position::new(0, 12),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 12),
                                    lsp::Position::new(0, 15),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 25),
                                    lsp::Position::new(0, 28),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                        ],
                    },
                    None,
                    DiagnosticSourceKind::Pushed,
                    &[],
                    cx,
                )
                .unwrap()
        });
    });

    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_diagnostic(&GoToPreviousDiagnostic::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});
}

#[gpui::test]
async fn go_to_diagnostic(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    // Place the cursor inside the `def` diagnostic (`[12, 15)`) before any
    // diagnostic is active so we can later confirm that running `editor: go to
    // diagnostic` will activate this diagnostic instead of advancing to the
    // next one.
    cx.set_state(indoc! {"
        fn func(abc dˇef: i32) -> u32 {
        }
    "});

    // Set up the diagnostics:
    //
    // * `[11, 12)` (the space before `def`),
    // * `[12, 15)` (`def`),
    // * `[25, 28)` (`u32`).
    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store
                .update_diagnostics(
                    LanguageServerId(0),
                    lsp::PublishDiagnosticsParams {
                        uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                        version: None,
                        diagnostics: vec![
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 11),
                                    lsp::Position::new(0, 12),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 12),
                                    lsp::Position::new(0, 15),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                            lsp::Diagnostic {
                                range: lsp::Range::new(
                                    lsp::Position::new(0, 25),
                                    lsp::Position::new(0, 28),
                                ),
                                severity: Some(lsp::DiagnosticSeverity::ERROR),
                                ..Default::default()
                            },
                        ],
                    },
                    None,
                    DiagnosticSourceKind::Pushed,
                    &[],
                    cx,
                )
                .unwrap()
        });
    });

    executor.run_until_parked();

    // When the cursor is at an inactive diagnostic, cursor should be moved to
    // the start of that same diagnostic and activate it.
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc ˇdef: i32) -> u32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});

    // Manually move the cursor to a different, not yet active diagnostic to
    // confirm that using `editor: go to diagnostic` will now activate this one.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 26)..Point::new(0, 26)])
        });
    });

    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abc def: i32) -> ˇu32 {
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)])
        });
    });
    cx.update_editor(|editor, window, cx| {
        editor.go_to_diagnostic(&GoToDiagnostic::default(), window, cx);
    });
    cx.assert_editor_state(indoc! {"
        fn func(abcˇ def: i32) -> u32 {
        }
    "});
}

#[gpui::test]
async fn test_go_to_hunk(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod;

        const A: u32 = 42;

        fn main() {
            println!("hello");

            println!("world");
        }
        "#
    .unindent();

    // Edits are modified, removed, modified, added
    cx.set_state(
        &r#"
        use some::modified;

        ˇ
        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        //Wrap around the bottom of the buffer
        for _ in 0..3 {
            editor.go_to_next_hunk(&GoToHunk, window, cx);
        }
    });

    cx.assert_editor_state(
        &r#"
        ˇuse some::modified;


        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        //Wrap around the top of the buffer
        for _ in 0..2 {
            editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
        }
    });

    cx.assert_editor_state(
        &r#"
        use some::modified;


        fn main() {
        ˇ    println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
    });

    cx.assert_editor_state(
        &r#"
        use some::modified;

        ˇ
        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
    });

    cx.assert_editor_state(
        &r#"
        ˇuse some::modified;


        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        for _ in 0..2 {
            editor.go_to_prev_hunk(&GoToPreviousHunk, window, cx);
        }
    });

    cx.assert_editor_state(
        &r#"
        use some::modified;


        fn main() {
        ˇ    println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.fold(&Fold, window, cx);
    });

    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_hunk(&GoToHunk, window, cx);
    });

    cx.assert_editor_state(
        &r#"
        ˇuse some::modified;


        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );
}

#[test]
fn test_split_words() {
    fn split(text: &str) -> Vec<&str> {
        split_words(text).collect()
    }

    assert_eq!(split("HelloWorld"), &["Hello", "World"]);
    assert_eq!(split("hello_world"), &["hello_", "world"]);
    assert_eq!(split("_hello_world_"), &["_", "hello_", "world_"]);
    assert_eq!(split("Hello_World"), &["Hello_", "World"]);
    assert_eq!(split("helloWOrld"), &["hello", "WOrld"]);
    assert_eq!(split("helloworld"), &["helloworld"]);

    assert_eq!(split(":do_the_thing"), &[":", "do_", "the_", "thing"]);
}

#[test]
fn test_split_words_for_snippet_prefix() {
    fn split(text: &str) -> Vec<&str> {
        snippet_candidate_suffixes(text, &|c| c.is_alphanumeric() || c == '_').collect()
    }

    assert_eq!(split("HelloWorld"), &["HelloWorld"]);
    assert_eq!(split("hello_world"), &["hello_world"]);
    assert_eq!(split("_hello_world_"), &["_hello_world_"]);
    assert_eq!(split("Hello_World"), &["Hello_World"]);
    assert_eq!(split("helloWOrld"), &["helloWOrld"]);
    assert_eq!(split("helloworld"), &["helloworld"]);
    assert_eq!(
        split("this@is!@#$^many   . symbols"),
        &[
            "symbols",
            " symbols",
            ". symbols",
            " . symbols",
            "  . symbols",
            "   . symbols",
            "many   . symbols",
            "^many   . symbols",
            "$^many   . symbols",
            "#$^many   . symbols",
            "@#$^many   . symbols",
            "!@#$^many   . symbols",
            "is!@#$^many   . symbols",
            "@is!@#$^many   . symbols",
            "this@is!@#$^many   . symbols",
        ],
    );
    assert_eq!(split("a.s"), &["s", ".s", "a.s"]);
}

#[gpui::test]
async fn test_move_to_syntax_node_relative_jumps(tcx: &mut TestAppContext) {
    init_test(tcx, |_| {});

    let mut cx = EditorLspTestContext::new(
        Arc::into_inner(markdown_lang()).unwrap(),
        Default::default(),
        tcx,
    )
    .await;

    async fn assert(offset: i8, before: &str, after: &str, cx: &mut EditorLspTestContext) {
        let _state_context = cx.set_state(before);
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| editor.go_to_symbol_by_offset(window, cx, offset))
            .await
            .unwrap();
        cx.run_until_parked();
        cx.assert_editor_state(after);
    }

    const ABOVE: i8 = -1;
    const BELOW: i8 = 1;

    assert(
        ABOVE,
        indoc! {"
        # Foo

        ˇFoo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        ABOVE,
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        ˇ# Foo

        Foo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        # Foo

        ˇFoo foo foo

        # Bar

        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        ˇ# Bar

        Bar bar bar
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        # Foo

        Foo foo foo

        # Bar
        ˇ
        Bar bar bar
    "},
        indoc! {"
        # Foo

        Foo foo foo

        # Bar
        ˇ
        Bar bar bar
    "},
        &mut cx,
    )
    .await;
}

#[gpui::test]
async fn test_move_to_syntax_node_relative_dead_zone(tcx: &mut TestAppContext) {
    init_test(tcx, |_| {});

    let mut cx = EditorLspTestContext::new(
        Arc::into_inner(rust_lang()).unwrap(),
        Default::default(),
        tcx,
    )
    .await;

    async fn assert(offset: i8, before: &str, after: &str, cx: &mut EditorLspTestContext) {
        let _state_context = cx.set_state(before);
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| editor.go_to_symbol_by_offset(window, cx, offset))
            .await
            .unwrap();
        cx.run_until_parked();
        cx.assert_editor_state(after);
    }

    const ABOVE: i8 = -1;
    const BELOW: i8 = 1;

    assert(
        ABOVE,
        indoc! {"
        fn foo() {
            // foo fn
        }

        ˇ// this zone is not inside any top level outline node

        fn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        indoc! {"
        ˇfn foo() {
            // foo fn
        }

        // this zone is not inside any top level outline node

        fn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        &mut cx,
    )
    .await;

    assert(
        BELOW,
        indoc! {"
        fn foo() {
            // foo fn
        }

        ˇ// this zone is not inside any top level outline node

        fn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        indoc! {"
        fn foo() {
            // foo fn
        }

        // this zone is not inside any top level outline node

        ˇfn bar() {
            // bar fn
            let _ = 2;
        }
    "},
        &mut cx,
    )
    .await;
}

#[gpui::test]
async fn test_move_to_enclosing_bracket(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_typescript(Default::default(), cx).await;

    #[track_caller]
    fn assert(before: &str, after: &str, cx: &mut EditorLspTestContext) {
        let _state_context = cx.set_state(before);
        cx.run_until_parked();
        cx.update_editor(|editor, window, cx| {
            editor.move_to_enclosing_bracket(&MoveToEnclosingBracket, window, cx)
        });
        cx.run_until_parked();
        cx.assert_editor_state(after);
    }

    // Outside bracket jumps to outside of matching bracket
    assert("console.logˇ(var);", "console.log(var)ˇ;", &mut cx);
    assert("console.log(var)ˇ;", "console.logˇ(var);", &mut cx);

    // Inside bracket jumps to inside of matching bracket
    assert("console.log(ˇvar);", "console.log(varˇ);", &mut cx);
    assert("console.log(varˇ);", "console.log(ˇvar);", &mut cx);

    // When outside a bracket and inside, favor jumping to the inside bracket
    assert(
        "console.log('foo', [1, 2, 3]ˇ);",
        "console.log('foo', ˇ[1, 2, 3]);",
        &mut cx,
    );
    assert(
        "console.log(ˇ'foo', [1, 2, 3]);",
        "console.log('foo'ˇ, [1, 2, 3]);",
        &mut cx,
    );

    // Bias forward if two options are equally likely
    assert(
        "let result = curried_fun()ˇ();",
        "let result = curried_fun()()ˇ;",
        &mut cx,
    );

    // If directly adjacent to a smaller pair but inside a larger (not adjacent), pick the smaller
    assert(
        indoc! {"
            function test() {
                console.log('test')ˇ
            }"},
        indoc! {"
            function test() {
                console.logˇ('test')
            }"},
        &mut cx,
    );
}

#[gpui::test]
async fn test_move_to_enclosing_bracket_in_markdown_code_block(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let language_registry = Arc::new(language::LanguageRegistry::test(cx.executor()));
    language_registry.add(markdown_lang());
    language_registry.add(rust_lang());
    let buffer = cx.new(|cx| {
        let mut buffer = language::Buffer::local(
            indoc! {"
            ```rs
            impl Worktree {
                pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                }
            }
            ```
        "},
            cx,
        );
        buffer.set_language_registry(language_registry.clone());
        buffer.set_language(Some(markdown_lang()), cx);
        buffer
    });
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let editor = cx.add_window(|window, cx| build_editor(buffer.clone(), window, cx));
    cx.executor().run_until_parked();
    _ = editor.update(cx, |editor, window, cx| {
        // Case 1: Test outer enclosing brackets
        select_ranges(
            editor,
            &indoc! {"
                ```rs
                impl Worktree {
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                    }
                }ˇ
                ```
            "},
            window,
            cx,
        );
        editor.move_to_enclosing_bracket(&MoveToEnclosingBracket, window, cx);
        assert_text_with_selections(
            editor,
            &indoc! {"
                ```rs
                impl Worktree ˇ{
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                    }
                }
                ```
            "},
            cx,
        );
        // Case 2: Test inner enclosing brackets
        select_ranges(
            editor,
            &indoc! {"
                ```rs
                impl Worktree {
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> {
                    }ˇ
                }
                ```
            "},
            window,
            cx,
        );
        editor.move_to_enclosing_bracket(&MoveToEnclosingBracket, window, cx);
        assert_text_with_selections(
            editor,
            &indoc! {"
                ```rs
                impl Worktree {
                    pub async fn open_buffers(&self, path: &Path) -> impl Iterator<&Buffer> ˇ{
                    }
                }
                ```
            "},
            cx,
        );
    });
}

#[gpui::test]
async fn test_on_type_formatting_not_triggered(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let a = 5; }",
            "other.rs": "// Test file",
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "{".to_string(),
                    end: "}".to_string(),
                    close: true,
                    surround: true,
                    newline: true,
                }],
                disabled_scopes_by_bracket_ix: Vec::new(),
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_on_type_formatting_provider: Some(lsp::DocumentOnTypeFormattingOptions {
                    first_trigger_character: "{".to_string(),
                    more_trigger_character: None,
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

    let worktree_id = workspace.update_in(cx, |workspace, _, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor_handle = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();

    fake_server.set_request_handler::<lsp::request::OnTypeFormatting, _, _>(
        |params, _| async move {
            assert_eq!(
                params.text_document_position.text_document.uri,
                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
            );
            assert_eq!(
                params.text_document_position.position,
                lsp::Position::new(0, 21),
            );

            Ok(Some(vec![lsp::TextEdit {
                new_text: "]".to_string(),
                range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            }]))
        },
    );

    editor_handle.update_in(cx, |editor, window, cx| {
        window.focus(&editor.focus_handle(cx), cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 21)..Point::new(0, 20)])
        });
        editor.handle_input("{", window, cx);
    });

    cx.executor().run_until_parked();

    buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "fn main() { let a = {5}; }",
            "No extra braces from on type formatting should appear in the buffer"
        )
    });
}

#[gpui::test(iterations = 20, seeds(31))]
async fn test_on_type_formatting_is_applied_after_autoindent(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_on_type_formatting_provider: Some(lsp::DocumentOnTypeFormattingOptions {
                first_trigger_character: ".".to_string(),
                more_trigger_character: None,
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.update_buffer(|buffer, _| {
        // This causes autoindent to be async.
        buffer.set_sync_parse_timeout(None)
    });

    cx.set_state("fn c() {\n    d()ˇ\n}\n");
    cx.simulate_keystroke("\n");
    cx.run_until_parked();

    let buffer_cloned = cx.multibuffer(|multi_buffer, _| multi_buffer.as_singleton().unwrap());
    let mut request =
        cx.set_request_handler::<lsp::request::OnTypeFormatting, _, _>(move |_, _, mut cx| {
            let buffer_cloned = buffer_cloned.clone();
            async move {
                buffer_cloned.update(&mut cx, |buffer, _| {
                    assert_eq!(
                        buffer.text(),
                        "fn c() {\n    d()\n        .\n}\n",
                        "OnTypeFormatting should triggered after autoindent applied"
                    )
                });

                Ok(Some(vec![]))
            }
        });

    cx.simulate_keystroke(".");
    cx.run_until_parked();

    cx.assert_editor_state("fn c() {\n    d()\n        .ˇ\n}\n");
    assert!(request.next().await.is_some());
    request.close();
    assert!(request.next().await.is_none());
}

#[gpui::test]
async fn test_language_server_restart_due_to_settings_change(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let a = 5; }",
            "other.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;

    let server_restarts = Arc::new(AtomicUsize::new(0));
    let closure_restarts = Arc::clone(&server_restarts);
    let language_server_name = "test language server";
    let language_name: LanguageName = "Rust".into();

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: language_name.clone(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )));
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: language_server_name,
            initialization_options: Some(json!({
                "testOptionValue": true
            })),
            initializer: Some(Box::new(move |fake_server| {
                let task_restarts = Arc::clone(&closure_restarts);
                fake_server.set_request_handler::<lsp::request::Shutdown, _, _>(move |_, _| {
                    task_restarts.fetch_add(1, atomic::Ordering::Release);
                    futures::future::ready(Ok(()))
                });
            })),
            ..Default::default()
        },
    );

    let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let _buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let _fake_server = fake_servers.next().await.unwrap();
    update_test_language_settings(cx, &|language_settings| {
        language_settings.languages.0.insert(
            language_name.clone().0.to_string(),
            LanguageSettingsContent {
                tab_size: NonZeroU32::new(8),
                ..Default::default()
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        0,
        "Should not restart LSP server on an unrelated change"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            "Some other server name".into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: Some(json!({
                    "some other init value": false
                })),
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        0,
        "Should not restart LSP server on an unrelated LSP settings change"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            language_server_name.into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: Some(json!({
                    "anotherInitValue": false
                })),
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        1,
        "Should restart LSP server on a related LSP settings change"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            language_server_name.into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: Some(json!({
                    "anotherInitValue": false
                })),
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        1,
        "Should not restart LSP server on a related LSP settings change that is the same"
    );

    update_test_project_settings(cx, &|project_settings| {
        project_settings.lsp.0.insert(
            language_server_name.into(),
            LspSettings {
                binary: None,
                settings: None,
                initialization_options: None,
                enable_lsp_tasks: false,
                fetch: None,
            },
        );
    });
    cx.executor().run_until_parked();
    assert_eq!(
        server_restarts.load(atomic::Ordering::Acquire),
        2,
        "Should restart LSP server on another related LSP settings change"
    );
}

#[gpui::test]
async fn test_completions_with_additional_edits(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");
    let completion_item = lsp::CompletionItem {
        label: "some".into(),
        kind: Some(lsp::CompletionItemKind::SNIPPET),
        detail: Some("Wrap the expression in an `Option::Some`".to_string()),
        documentation: Some(lsp::Documentation::MarkupContent(lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: "```rust\nSome(2)\n```".to_string(),
        })),
        deprecated: Some(false),
        sort_text: Some("fffffff2".to_string()),
        filter_text: Some("some".to_string()),
        insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 22,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "Some(2)".to_string(),
        })),
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 20,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "".to_string(),
        }]),
        ..Default::default()
    };

    let closure_completion_item = completion_item.clone();
    let mut request = cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let task_completion_item = closure_completion_item.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                task_completion_item,
            ])))
        }
    });

    request.next().await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion(&ConfirmCompletion::default(), window, cx)
            .unwrap()
    });
    cx.assert_editor_state("fn main() { let a = 2.Some(2)ˇ; }");

    cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(move |_, _, _| {
        let task_completion_item = completion_item.clone();
        async move { Ok(task_completion_item) }
    })
    .next()
    .await
    .unwrap();
    apply_additional_edits.await.unwrap();
    cx.assert_editor_state("fn main() { let a = Some(2)ˇ; }");
}

#[gpui::test]
async fn test_completions_with_additional_edits_undo(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");
    let completion_item = lsp::CompletionItem {
        label: "some".into(),
        kind: Some(lsp::CompletionItemKind::SNIPPET),
        detail: Some("Wrap the expression in an `Option::Some`".to_string()),
        documentation: Some(lsp::Documentation::MarkupContent(lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: "```rust\nSome(2)\n```".to_string(),
        })),
        deprecated: Some(false),
        sort_text: Some("fffffff2".to_string()),
        filter_text: Some("some".to_string()),
        insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 22,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "Some(2)".to_string(),
        })),
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 20,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "".to_string(),
        }]),
        ..Default::default()
    };

    let closure_completion_item = completion_item.clone();
    let mut request = cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let task_completion_item = closure_completion_item.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                task_completion_item,
            ])))
        }
    });

    request.next().await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion(&ConfirmCompletion::default(), window, cx)
            .unwrap()
    });
    cx.assert_editor_state("fn main() { let a = 2.Some(2)ˇ; }");

    cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(move |_, _, _| {
        let task_completion_item = completion_item.clone();
        async move { Ok(task_completion_item) }
    })
    .next()
    .await
    .unwrap();
    apply_additional_edits.await.unwrap();
    cx.assert_editor_state("fn main() { let a = Some(2)ˇ; }");

    cx.update_editor(|editor, window, cx| {
        editor.undo(&crate::Undo, window, cx);
    });
    cx.assert_editor_state("fn main() { let a = 2.ˇ; }");
}

#[gpui::test]
async fn test_completions_with_additional_edits_and_multiple_cursors(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(
        "import { «Fooˇ» } from './types';\n\nclass Bar {\n    method(): «Fooˇ» { return new Foo(); }\n}",
    );

    cx.simulate_keystroke("F");
    cx.simulate_keystroke("o");

    let completion_item = lsp::CompletionItem {
        label: "FooBar".into(),
        kind: Some(lsp::CompletionItemKind::CLASS),
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 3,
                    character: 14,
                },
                end: lsp::Position {
                    line: 3,
                    character: 16,
                },
            },
            new_text: "FooBar".to_string(),
        })),
        additional_text_edits: Some(vec![lsp::TextEdit {
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
            new_text: "FooBar".to_string(),
        }]),
        ..Default::default()
    };

    let closure_completion_item = completion_item.clone();
    let mut request = cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let task_completion_item = closure_completion_item.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                task_completion_item,
            ])))
        }
    });

    request.next().await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion(&ConfirmCompletion::default(), window, cx)
            .unwrap()
    });

    cx.assert_editor_state(
        "import { FooBarˇ } from './types';\n\nclass Bar {\n    method(): FooBarˇ { return new Foo(); }\n}",
    );

    cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(move |_, _, _| {
        let task_completion_item = completion_item.clone();
        async move { Ok(task_completion_item) }
    })
    .next()
    .await
    .unwrap();

    apply_additional_edits.await.unwrap();

    cx.assert_editor_state(
        "import { FooBarˇ } from './types';\n\nclass Bar {\n    method(): FooBarˇ { return new Foo(); }\n}",
    );
}

#[gpui::test]
async fn test_completions_resolve_updates_labels_if_filter_text_matches(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");

    let item1 = lsp::CompletionItem {
        label: "method id()".to_string(),
        filter_text: Some("id".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".id".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };

    let item2 = lsp::CompletionItem {
        label: "other".to_string(),
        filter_text: Some("other".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".other".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };

    let item1 = item1.clone();
    cx.set_request_handler::<lsp::request::Completion, _, _>({
        let item1 = item1.clone();
        move |_, _, _| {
            let item1 = item1.clone();
            let item2 = item2.clone();
            async move { Ok(Some(lsp::CompletionResponse::Array(vec![item1, item2]))) }
        }
    })
    .next()
    .await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        let context_menu = editor.context_menu.borrow_mut();
        let context_menu = context_menu
            .as_ref()
            .expect("Should have the context menu deployed");
        match context_menu {
            CodeContextMenu::Completions(completions_menu) => {
                let completions = completions_menu.completions.borrow_mut();
                assert_eq!(
                    completions
                        .iter()
                        .map(|completion| &completion.label.text)
                        .collect::<Vec<_>>(),
                    vec!["method id()", "other"]
                )
            }
            CodeContextMenu::CodeActions(_) => panic!("Should show the completions menu"),
        }
    });

    cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>({
        let item1 = item1.clone();
        move |_, item_to_resolve, _| {
            let item1 = item1.clone();
            async move {
                if item1 == item_to_resolve {
                    Ok(lsp::CompletionItem {
                        label: "method id()".to_string(),
                        filter_text: Some("id".to_string()),
                        detail: Some("Now resolved!".to_string()),
                        documentation: Some(lsp::Documentation::String("Docs".to_string())),
                        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                            range: lsp::Range::new(
                                lsp::Position::new(0, 22),
                                lsp::Position::new(0, 22),
                            ),
                            new_text: ".id".to_string(),
                        })),
                        ..lsp::CompletionItem::default()
                    })
                } else {
                    Ok(item_to_resolve)
                }
            }
        }
    })
    .next()
    .await
    .unwrap();
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&Default::default(), window, cx);
    });
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        let context_menu = editor.context_menu.borrow_mut();
        let context_menu = context_menu
            .as_ref()
            .expect("Should have the context menu deployed");
        match context_menu {
            CodeContextMenu::Completions(completions_menu) => {
                let completions = completions_menu.completions.borrow_mut();
                assert_eq!(
                    completions
                        .iter()
                        .map(|completion| &completion.label.text)
                        .collect::<Vec<_>>(),
                    vec!["method id() Now resolved!", "other"],
                    "Should update first completion label, but not second as the filter text did not match."
                );
            }
            CodeContextMenu::CodeActions(_) => panic!("Should show the completions menu"),
        }
    });
}

#[gpui::test]
async fn test_context_menus_hide_hover_popover(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
            completion_provider: Some(lsp::CompletionOptions {
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;
    cx.set_state(indoc! {"
        struct TestStruct {
            field: i32
        }

        fn mainˇ() {
            let unused_var = 42;
            let test_struct = TestStruct { field: 42 };
        }
    "});
    let symbol_range = cx.lsp_range(indoc! {"
        struct TestStruct {
            field: i32
        }

        «fn main»() {
            let unused_var = 42;
            let test_struct = TestStruct { field: 42 };
        }
    "});
    let mut hover_requests =
        cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| async move {
            Ok(Some(lsp::Hover {
                contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: "Function documentation".to_string(),
                }),
                range: Some(symbol_range),
            }))
        });

    // Case 1: Test that code action menu hide hover popover
    cx.dispatch_action(Hover);
    hover_requests.next().await;
    cx.condition(|editor, _| editor.hover_state.visible()).await;
    let mut code_action_requests = cx.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                lsp::CodeAction {
                    title: "Remove unused variable".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(lsp::WorkspaceEdit {
                        changes: Some(
                            [(
                                lsp::Uri::from_file_path(path!("/file.rs")).unwrap(),
                                vec![lsp::TextEdit {
                                    range: lsp::Range::new(
                                        lsp::Position::new(5, 4),
                                        lsp::Position::new(5, 27),
                                    ),
                                    new_text: "".to_string(),
                                }],
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]))
        },
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_code_actions(
            &ToggleCodeActions {
                deployed_from: None,
                quick_launch: false,
            },
            window,
            cx,
        );
    });
    code_action_requests.next().await;
    cx.run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Hover popover should be hidden when code action menu is shown"
        );
        // Hide code actions
        editor.context_menu.take();
    });

    // Case 2: Test that code completions hide hover popover
    cx.dispatch_action(Hover);
    hover_requests.next().await;
    cx.condition(|editor, _| editor.hover_state.visible()).await;
    let counter = Arc::new(AtomicUsize::new(0));
    let mut completion_requests =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, atomic::Ordering::Release);
                Ok(Some(lsp::CompletionResponse::Array(vec![
                    lsp::CompletionItem {
                        label: "main".into(),
                        kind: Some(lsp::CompletionItemKind::FUNCTION),
                        detail: Some("() -> ()".to_string()),
                        ..Default::default()
                    },
                    lsp::CompletionItem {
                        label: "TestStruct".into(),
                        kind: Some(lsp::CompletionItemKind::STRUCT),
                        detail: Some("struct TestStruct".to_string()),
                        ..Default::default()
                    },
                ])))
            }
        });
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    completion_requests.next().await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Hover popover should be hidden when completion menu is shown"
        );
    });
}

#[gpui::test]
async fn test_completions_resolve_happens_once(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");

    let unresolved_item_1 = lsp::CompletionItem {
        label: "id".to_string(),
        filter_text: Some("id".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".id".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };
    let resolved_item_1 = lsp::CompletionItem {
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 20), lsp::Position::new(0, 22)),
            new_text: "!!".to_string(),
        }]),
        ..unresolved_item_1.clone()
    };
    let unresolved_item_2 = lsp::CompletionItem {
        label: "other".to_string(),
        filter_text: Some("other".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".other".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };
    let resolved_item_2 = lsp::CompletionItem {
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 20), lsp::Position::new(0, 22)),
            new_text: "??".to_string(),
        }]),
        ..unresolved_item_2.clone()
    };

    let resolve_requests_1 = Arc::new(AtomicUsize::new(0));
    let resolve_requests_2 = Arc::new(AtomicUsize::new(0));
    cx.lsp
        .server
        .on_request::<lsp::request::ResolveCompletionItem, _, _>({
            let unresolved_item_1 = unresolved_item_1.clone();
            let resolved_item_1 = resolved_item_1.clone();
            let unresolved_item_2 = unresolved_item_2.clone();
            let resolved_item_2 = resolved_item_2.clone();
            let resolve_requests_1 = resolve_requests_1.clone();
            let resolve_requests_2 = resolve_requests_2.clone();
            move |unresolved_request, _| {
                let unresolved_item_1 = unresolved_item_1.clone();
                let resolved_item_1 = resolved_item_1.clone();
                let unresolved_item_2 = unresolved_item_2.clone();
                let resolved_item_2 = resolved_item_2.clone();
                let resolve_requests_1 = resolve_requests_1.clone();
                let resolve_requests_2 = resolve_requests_2.clone();
                async move {
                    if unresolved_request == unresolved_item_1 {
                        resolve_requests_1.fetch_add(1, atomic::Ordering::Release);
                        Ok(resolved_item_1.clone())
                    } else if unresolved_request == unresolved_item_2 {
                        resolve_requests_2.fetch_add(1, atomic::Ordering::Release);
                        Ok(resolved_item_2.clone())
                    } else {
                        panic!("Unexpected completion item {unresolved_request:?}")
                    }
                }
            }
        })
        .detach();

    cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let unresolved_item_1 = unresolved_item_1.clone();
        let unresolved_item_2 = unresolved_item_2.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                unresolved_item_1,
                unresolved_item_2,
            ])))
        }
    })
    .next()
    .await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        let context_menu = editor.context_menu.borrow_mut();
        let context_menu = context_menu
            .as_ref()
            .expect("Should have the context menu deployed");
        match context_menu {
            CodeContextMenu::Completions(completions_menu) => {
                let completions = completions_menu.completions.borrow_mut();
                assert_eq!(
                    completions
                        .iter()
                        .map(|completion| &completion.label.text)
                        .collect::<Vec<_>>(),
                    vec!["id", "other"]
                )
            }
            CodeContextMenu::CodeActions(_) => panic!("Should show the completions menu"),
        }
    });
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&ContextMenuNext, window, cx);
    });
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.context_menu_prev(&ContextMenuPrevious, window, cx);
    });
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&ContextMenuNext, window, cx);
    });
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor
            .compose_completion(&ComposeCompletion::default(), window, cx)
            .expect("No task returned")
    })
    .await
    .expect("Completion failed");
    cx.run_until_parked();

    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            resolve_requests_1.load(atomic::Ordering::Acquire),
            1,
            "Should always resolve once despite multiple selections"
        );
        assert_eq!(
            resolve_requests_2.load(atomic::Ordering::Acquire),
            1,
            "Should always resolve once after multiple selections and applying the completion"
        );
        assert_eq!(
            editor.text(cx),
            "fn main() { let a = ??.other; }",
            "Should use resolved data when applying the completion"
        );
    });
}

#[gpui::test]
async fn test_completions_default_resolve_data_handling(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let item_0 = lsp::CompletionItem {
        label: "abs".into(),
        insert_text: Some("abs".into()),
        data: Some(json!({ "very": "special"})),
        insert_text_mode: Some(lsp::InsertTextMode::ADJUST_INDENTATION),
        text_edit: Some(lsp::CompletionTextEdit::InsertAndReplace(
            lsp::InsertReplaceEdit {
                new_text: "abs".to_string(),
                insert: lsp::Range::default(),
                replace: lsp::Range::default(),
            },
        )),
        ..lsp::CompletionItem::default()
    };
    let items = iter::once(item_0.clone())
        .chain((11..51).map(|i| lsp::CompletionItem {
            label: format!("item_{}", i),
            insert_text: Some(format!("item_{}", i)),
            insert_text_format: Some(lsp::InsertTextFormat::PLAIN_TEXT),
            ..lsp::CompletionItem::default()
        }))
        .collect::<Vec<_>>();

    let default_commit_characters = vec!["?".to_string()];
    let default_data = json!({ "default": "data"});
    let default_insert_text_format = lsp::InsertTextFormat::SNIPPET;
    let default_insert_text_mode = lsp::InsertTextMode::AS_IS;
    let default_edit_range = lsp::Range {
        start: lsp::Position {
            line: 0,
            character: 5,
        },
        end: lsp::Position {
            line: 0,
            character: 5,
        },
    };

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");

    let completion_data = default_data.clone();
    let completion_characters = default_commit_characters.clone();
    let completion_items = items.clone();
    cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let default_data = completion_data.clone();
        let default_commit_characters = completion_characters.clone();
        let items = completion_items.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                items,
                item_defaults: Some(lsp::CompletionListItemDefaults {
                    data: Some(default_data.clone()),
                    commit_characters: Some(default_commit_characters.clone()),
                    edit_range: Some(lsp::CompletionListItemDefaultsEditRange::Range(
                        default_edit_range,
                    )),
                    insert_text_format: Some(default_insert_text_format),
                    insert_text_mode: Some(default_insert_text_mode),
                }),
                ..lsp::CompletionList::default()
            })))
        }
    })
    .next()
    .await;

    let resolved_items = Arc::new(Mutex::new(Vec::new()));
    cx.lsp
        .server
        .on_request::<lsp::request::ResolveCompletionItem, _, _>({
            let closure_resolved_items = resolved_items.clone();
            move |item_to_resolve, _| {
                let closure_resolved_items = closure_resolved_items.clone();
                async move {
                    closure_resolved_items.lock().push(item_to_resolve.clone());
                    Ok(item_to_resolve)
                }
            }
        })
        .detach();

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.run_until_parked();
    cx.update_editor(|editor, _, _| {
        let menu = editor.context_menu.borrow_mut();
        match menu.as_ref().expect("should have the completions menu") {
            CodeContextMenu::Completions(completions_menu) => {
                assert_eq!(
                    completions_menu
                        .entries
                        .borrow()
                        .iter()
                        .filter_map(|entry| entry.as_match().map(|m| m.string.clone()))
                        .collect::<Vec<String>>(),
                    items
                        .iter()
                        .map(|completion| completion.label.clone())
                        .collect::<Vec<String>>()
                );
            }
            CodeContextMenu::CodeActions(_) => panic!("Expected to have the completions menu"),
        }
    });
    // Approximate initial displayed interval is 0..12. With extra item padding of 4 this is 0..16
    // with 4 from the end.
    assert_eq!(
        *resolved_items.lock(),
        [&items[0..16], &items[items.len() - 4..items.len()]]
            .concat()
            .iter()
            .cloned()
            .map(|mut item| {
                if item.data.is_none() {
                    item.data = Some(default_data.clone());
                }
                item
            })
            .collect::<Vec<lsp::CompletionItem>>(),
        "Items sent for resolve should be unchanged modulo resolve `data` filled with default if missing"
    );
    resolved_items.lock().clear();

    cx.update_editor(|editor, window, cx| {
        editor.context_menu_prev(&ContextMenuPrevious, window, cx);
    });
    cx.run_until_parked();
    // Completions that have already been resolved are skipped.
    assert_eq!(
        *resolved_items.lock(),
        items[items.len() - 17..items.len() - 4]
            .iter()
            .cloned()
            .map(|mut item| {
                if item.data.is_none() {
                    item.data = Some(default_data.clone());
                }
                item
            })
            .collect::<Vec<lsp::CompletionItem>>()
    );
    resolved_items.lock().clear();
}

#[gpui::test]
async fn test_completions_in_languages_with_extra_word_characters(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new(
        Language::new(
            LanguageConfig {
                matcher: LanguageMatcher {
                    path_suffixes: vec!["jsx".into()],
                    ..Default::default()
                },
                overrides: [(
                    "element".into(),
                    LanguageConfigOverride {
                        completion_query_characters: Override::Set(['-'].into_iter().collect()),
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        )
        .with_override_query("(jsx_self_closing_element) @element")
        .unwrap(),
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![":".to_string()]),
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
                    label: "bg-blue".into(),
                    ..Default::default()
                },
                lsp::CompletionItem {
                    label: "bg-red".into(),
                    ..Default::default()
                },
                lsp::CompletionItem {
                    label: "bg-yellow".into(),
                    ..Default::default()
                },
            ])))
        });

    cx.set_state(r#"<p class="bgˇ" />"#);

    // Trigger completion when typing a dash, because the dash is an extra
    // word character in the 'element' scope, which contains the cursor.
    cx.simulate_keystroke("-");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["bg-blue", "bg-red", "bg-yellow"]
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });

    cx.simulate_keystroke("l");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["bg-blue", "bg-yellow"]);
        } else {
            panic!("expected completion menu to be open");
        }
    });

    // When filtering completions, consider the character after the '-' to
    // be the start of a subword.
    cx.set_state(r#"<p class="yelˇ" />"#);
    cx.simulate_keystroke("l");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["bg-yellow"]);
        } else {
            panic!("expected completion menu to be open");
        }
    });
}

fn completion_menu_entries(menu: &CompletionsMenu) -> Vec<String> {
    let entries = menu.entries.borrow();
    entries
        .iter()
        .filter_map(|entry| entry.as_match().map(|m| m.string.clone()))
        .collect()
}

#[gpui::test]
async fn test_document_format_with_prettier(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.ts"), Default::default()).await;

    let project = Project::test(fs, [path!("/file.ts").as_ref()], cx).await;
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
    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_format_suffix = project::TEST_PRETTIER_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.ts"), cx)
        })
        .await
        .unwrap();

    let buffer_text = "one\ntwo\nthree\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project.clone(),
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap()
        .await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string() + prettier_format_suffix,
        "Test prettier formatting was not applied to the original buffer text",
    );

    update_test_language_settings(cx, &|settings| {
        settings.defaults.formatter = Some(FormatterList::default())
    });
    let format = editor.update_in(cx, |editor, window, cx| {
        editor.perform_format(
            project.clone(),
            FormatTrigger::Manual,
            FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
            window,
            cx,
        )
    });
    format.await.unwrap();
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string() + prettier_format_suffix + "\n" + prettier_format_suffix,
        "Autoformatting (via test prettier) was not applied to the original buffer text",
    );
}

#[gpui::test]
async fn test_document_format_with_prettier_explicit_language(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.settings"), Default::default())
        .await;

    let project = Project::test(fs, [path!("/file.settings").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let ts_lang = Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..LanguageMatcher::default()
            },
            prettier_parser_name: Some("typescript".to_string()),
            ..LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    ));

    language_registry.add(ts_lang.clone());

    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_format_suffix = project::TEST_PRETTIER_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.settings"), cx)
        })
        .await
        .unwrap();

    project.update(cx, |project, cx| {
        project.set_language_for_buffer(&buffer, ts_lang, cx)
    });

    let buffer_text = "one\ntwo\nthree\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project.clone(),
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap()
        .await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string() + prettier_format_suffix + "\ntypescript",
        "Test prettier formatting was not applied to the original buffer text",
    );

    update_test_language_settings(cx, &|settings| {
        settings.defaults.formatter = Some(FormatterList::default())
    });
    let format = editor.update_in(cx, |editor, window, cx| {
        editor.perform_format(
            project.clone(),
            FormatTrigger::Manual,
            FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
            window,
            cx,
        )
    });
    format.await.unwrap();

    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        buffer_text.to_string()
            + prettier_format_suffix
            + "\ntypescript\n"
            + prettier_format_suffix
            + "\ntypescript",
        "Autoformatting (via test prettier) was not applied to the original buffer text",
    );
}

#[gpui::test]
async fn test_range_format_with_prettier(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.ts"), Default::default()).await;

    let project = Project::test(fs, [path!("/file.ts").as_ref()], cx).await;
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
    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_range_format_suffix = project::TEST_PRETTIER_RANGE_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.ts"), cx)
        })
        .await
        .unwrap();

    let buffer_text = "one\ntwo\nthree\nfour\nfive\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(3, 0)])
        });
    });

    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.format_selections(&FormatSelections, window, cx)
        })
        .unwrap();
    format.await.unwrap();

    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        format!("one\ntwo{prettier_range_format_suffix}\nthree\nfour\nfive\n"),
        "Range formatting (via test prettier) was not applied to the buffer text",
    );
}

#[gpui::test]
async fn test_range_format_with_prettier_explicit_language(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Single(Formatter::Prettier))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.settings"), Default::default())
        .await;

    let project = Project::test(fs, [path!("/file.settings").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let ts_lang = Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..LanguageMatcher::default()
            },
            prettier_parser_name: Some("typescript".to_string()),
            ..LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    ));

    language_registry.add(ts_lang.clone());

    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });

    let test_plugin = "test_plugin";
    let _ = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            prettier_plugins: vec![test_plugin],
            ..Default::default()
        },
    );

    let prettier_range_format_suffix = project::TEST_PRETTIER_RANGE_FORMAT_SUFFIX;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.settings"), cx)
        })
        .await
        .unwrap();

    project.update(cx, |project, cx| {
        project.set_language_for_buffer(&buffer, ts_lang, cx)
    });

    let buffer_text = "one\ntwo\nthree\nfour\nfive\n";
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(buffer_text, window, cx)
    });

    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::default(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(3, 0)])
        });
    });

    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.format_selections(&FormatSelections, window, cx)
        })
        .unwrap();
    format.await.unwrap();

    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        format!("one\ntwo{prettier_range_format_suffix}\ntypescript\nthree\nfour\nfive\n"),
        "Range formatting (via test prettier) was not applied with explicit language",
    );
}

#[gpui::test]
async fn test_addition_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"
        struct Row;
        struct Row1;
        struct Row2;

        struct Row4;
        struct Row5;
        struct Row6;

        struct Row8;
        struct Row9;
        struct Row10;"#};

    // When addition hunks are not adjacent to carets, no hunk revert is performed
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row1.1;
                   struct Row1.2;
                   struct Row2;ˇ

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;
                   ˇstruct Row9;
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Added, DiffHunkStatusKind::Added],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row1.1;
                   struct Row1.2;
                   struct Row2;ˇ

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;
                   ˇstruct Row9;
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    // Same for selections
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row2;
                   struct Row2.1;
                   struct Row2.2;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row8;
                   struct Row9;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Added, DiffHunkStatusKind::Added],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row2;
                   struct Row2.1;
                   struct Row2.2;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row8;
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );

    // When carets and selections intersect the addition hunks, those are reverted.
    // Adjacent carets got merged.
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   ˇ// something on the top
                   struct Row1;
                   struct Row2;
                   struct Roˇw3.1;
                   struct Row2.2;
                   struct Row2.3;ˇ

                   struct Row4;
                   struct ˇRow5.1;
                   struct Row5.2;
                   struct «Rowˇ»5.3;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row9.1;
                   struct «Rowˇ»9.2;
                   struct «ˇRow»9.3;
                   struct Row8;
                   struct Row9;
                   «ˇ// something on bottom»
                   struct Row10;"#},
        vec![
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
        ],
        indoc! {r#"struct Row;
                   ˇstruct Row1;
                   struct Row2;
                   ˇ
                   struct Row4;
                   ˇstruct Row5;
                   struct Row6;
                   ˇ
                   ˇstruct Row8;
                   struct Row9;
                   ˇstruct Row10;"#},
        base_text,
        &mut cx,
    );
}

#[gpui::test]
async fn test_modification_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"
        struct Row;
        struct Row1;
        struct Row2;

        struct Row4;
        struct Row5;
        struct Row6;

        struct Row8;
        struct Row9;
        struct Row10;"#};

    // Modification hunks behave the same as the addition ones.
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   ˇ
                   struct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Modified, DiffHunkStatusKind::Modified],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   ˇ
                   struct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Modified, DiffHunkStatusKind::Modified],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );

    assert_hunk_revert(
        indoc! {r#"ˇstruct Row1.1;
                   struct Row1;
                   «ˇstr»uct Row22;

                   struct ˇRow44;
                   struct Row5;
                   struct «Rˇ»ow66;ˇ

                   «struˇ»ct Row88;
                   struct Row9;
                   struct Row1011;ˇ"#},
        vec![
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
        ],
        indoc! {r#"struct Row;
                   ˇstruct Row1;
                   struct Row2;
                   ˇ
                   struct Row4;
                   ˇstruct Row5;
                   struct Row6;
                   ˇ
                   struct Row8;
                   ˇstruct Row9;
                   struct Row10;ˇ"#},
        base_text,
        &mut cx,
    );
}

#[gpui::test]
async fn test_deleting_over_diff_hunk(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"
        one

        two
        three
        "#};

    cx.set_head_text(base_text);
    cx.set_state("\nˇ\n");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _window, cx| {
        editor.expand_selected_diff_hunks(cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_state_with_diff(
        indoc! {r#"

        - two
        - threeˇ
        +
        "#}
        .to_string(),
    );
}

#[gpui::test]
async fn test_deletion_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"struct Row;
struct Row1;
struct Row2;

struct Row4;
struct Row5;
struct Row6;

struct Row8;
struct Row9;
struct Row10;"#};

    // Deletion hunks trigger with carets on adjacent rows, so carets and selections have to stay farther to avoid the revert
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row2;

                   ˇstruct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row8;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Deleted, DiffHunkStatusKind::Deleted],
        indoc! {r#"struct Row;
                   struct Row2;

                   ˇstruct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row8;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row2;

                   «ˇstruct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row8;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Deleted, DiffHunkStatusKind::Deleted],
        indoc! {r#"struct Row;
                   struct Row2;

                   «ˇstruct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row8;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );

    // Deletion hunks are ephemeral, so it's impossible to place the caret into them — Mav triggers reverts for lines, adjacent to carets and selections.
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   ˇstruct Row2;

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;ˇ
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Deleted, DiffHunkStatusKind::Deleted],
        indoc! {r#"struct Row;
                   struct Row1;
                   ˇstruct Row2;

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;ˇ
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row2«ˇ;
                   struct Row4;
                   struct» Row5;
                   «struct Row6;

                   struct Row8;ˇ»
                   struct Row10;"#},
        vec![
            DiffHunkStatusKind::Deleted,
            DiffHunkStatusKind::Deleted,
            DiffHunkStatusKind::Deleted,
        ],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row2«ˇ;

                   struct Row4;
                   struct» Row5;
                   «struct Row6;

                   struct Row8;ˇ»
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
}

#[gpui::test]
async fn test_multibuffer_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let base_text_1 = "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj";
    let base_text_2 = "llll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu";
    let base_text_3 =
        "vvvv\nwwww\nxxxx\nyyyy\nzzzz\n{{{{\n||||\n}}}}\n~~~~\n\u{7f}\u{7f}\u{7f}\u{7f}";

    let text_1 = edit_first_char_of_every_line(base_text_1);
    let text_2 = edit_first_char_of_every_line(base_text_2);
    let text_3 = edit_first_char_of_every_line(base_text_3);

    let buffer_1 = cx.new(|cx| Buffer::local(text_1.clone(), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(text_2.clone(), cx));
    let buffer_3 = cx.new(|cx| Buffer::local(text_3.clone(), cx));

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [path!("/").as_ref()], cx).await;
    let (editor, cx) = cx
        .add_window_view(|window, cx| build_editor_with_project(project, multibuffer, window, cx));
    editor.update_in(cx, |editor, _window, cx| {
        for (buffer, diff_base) in [
            (buffer_1.clone(), base_text_1),
            (buffer_2.clone(), base_text_2),
            (buffer_3.clone(), base_text_3),
        ] {
            let diff = cx.new(|cx| {
                BufferDiff::new_with_base_text(diff_base, &buffer.read(cx).text_snapshot(), cx)
            });
            editor
                .buffer
                .update(cx, |buffer, cx| buffer.add_diff(diff, cx));
        }
    });
    cx.executor().run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(editor.display_text(cx), "\n\nXaaa\nXbbb\nXccc\n\nXfff\nXggg\n\nXjjj\n\n\nXlll\nXmmm\nXnnn\n\nXqqq\nXrrr\n\nXuuu\n\n\nXvvv\nXwww\nXxxx\n\nX{{{\nX|||\n\nX\u{7f}\u{7f}\u{7f}");
        editor.select_all(&SelectAll, window, cx);
        editor.git_restore(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    // When all ranges are selected, all buffer hunks are reverted.
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.display_text(cx), "\n\naaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj\n\n\n\n\n\n\nllll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu\n\n\n\n\n\n\nvvvv\nwwww\nxxxx\nyyyy\nzzzz\n{{{{\n||||\n}}}}\n~~~~\n\u{7f}\u{7f}\u{7f}\u{7f}\n\n\n\n");
    });
    buffer_1.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_1);
    });
    buffer_2.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_2);
    });
    buffer_3.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_3);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.undo(&Default::default(), window, cx);
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(Point::new(0, 0)..Point::new(5, 0)));
        });
        editor.git_restore(&Default::default(), window, cx);
    });

    // Now, when all ranges selected belong to buffer_1, the revert should succeed,
    // but not affect buffer_2 and its related excerpts.
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "\n\naaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj\n\n\n\n\n\n\nXlll\nXmmm\nXnnn\n\nXqqq\nXrrr\n\nXuuu\n\n\nXvvv\nXwww\nXxxx\n\nX{{{\nX|||\n\nX\u{7f}\u{7f}\u{7f}"
        );
    });
    buffer_1.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), base_text_1);
    });
    buffer_2.update(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "Xlll\nXmmm\nXnnn\nXooo\nXppp\nXqqq\nXrrr\nXsss\nXttt\nXuuu"
        );
    });
    buffer_3.update(cx, |buffer, _| {
        assert_eq!(
            buffer.text(),
            "Xvvv\nXwww\nXxxx\nXyyy\nXzzz\nX{{{\nX|||\nX}}}\nX~~~\nX\u{7f}\u{7f}\u{7f}"
        );
    });

    fn edit_first_char_of_every_line(text: &str) -> String {
        text.split('\n')
            .map(|line| format!("X{}", &line[1..]))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[gpui::test]
async fn test_multibuffer_in_navigation_history(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let cols = 4;
    let rows = 10;
    let sample_text_1 = sample_text(rows, cols, 'a');
    assert_eq!(
        sample_text_1,
        "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj"
    );
    let sample_text_2 = sample_text(rows, cols, 'l');
    assert_eq!(
        sample_text_2,
        "llll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu"
    );
    let sample_text_3 = sample_text(rows, cols, 'v');
    assert_eq!(
        sample_text_3,
        "vvvv\nwwww\nxxxx\nyyyy\nzzzz\n{{{{\n||||\n}}}}\n~~~~\n\u{7f}\u{7f}\u{7f}\u{7f}"
    );

    let buffer_1 = cx.new(|cx| Buffer::local(sample_text_1.clone(), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(sample_text_2.clone(), cx));
    let buffer_3 = cx.new(|cx| Buffer::local(sample_text_3.clone(), cx));

    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(9, 4),
            ],
            0,
            cx,
        );
        multibuffer
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/a",
        json!({
            "main.rs": sample_text_1,
            "other.rs": sample_text_2,
            "lib.rs": sample_text_3,
        }),
    )
    .await;
    let project = Project::test(fs, ["/a".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });
    let multibuffer_item_id = workspace.update_in(cx, |workspace, window, cx| {
        assert!(
            workspace.active_item(cx).is_none(),
            "active item should be None before the first item is added"
        );
        workspace.add_item_to_active_pane(
            Box::new(multi_buffer_editor.clone()),
            None,
            true,
            window,
            cx,
        );
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after adding the multi buffer");
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Multibuffer,
            "A multi buffer was expected to active after adding"
        );
        active_item.item_id()
    });

    cx.executor().run_until_parked();

    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(1)..MultiBufferOffset(2))),
        );
        editor.open_excerpts(&OpenExcerpts, window, cx);
    });
    cx.executor().run_until_parked();
    let first_item_id = workspace.update_in(cx, |workspace, window, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating into the 1st buffer");
        let first_item_id = active_item.item_id();
        assert_ne!(
            first_item_id, multibuffer_item_id,
            "Should navigate into the 1st buffer and activate it"
        );
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Singleton,
            "New active item should be a singleton buffer"
        );
        assert_eq!(
            active_item
                .act_as::<Editor>(cx)
                .expect("should have navigated into an editor for the 1st buffer")
                .read(cx)
                .text(cx),
            sample_text_1
        );

        workspace
            .go_back(workspace.active_pane().downgrade(), window, cx)
            .detach_and_log_err(cx);

        first_item_id
    });

    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, _, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating back");
        assert_eq!(
            active_item.item_id(),
            multibuffer_item_id,
            "Should navigate back to the multi buffer"
        );
        assert_eq!(active_item.buffer_kind(cx), ItemBufferKind::Multibuffer);
    });

    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(39)..MultiBufferOffset(40))),
        );
        editor.open_excerpts(&OpenExcerpts, window, cx);
    });
    cx.executor().run_until_parked();
    let second_item_id = workspace.update_in(cx, |workspace, window, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating into the 2nd buffer");
        let second_item_id = active_item.item_id();
        assert_ne!(
            second_item_id, multibuffer_item_id,
            "Should navigate away from the multibuffer"
        );
        assert_ne!(
            second_item_id, first_item_id,
            "Should navigate into the 2nd buffer and activate it"
        );
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Singleton,
            "New active item should be a singleton buffer"
        );
        assert_eq!(
            active_item
                .act_as::<Editor>(cx)
                .expect("should have navigated into an editor")
                .read(cx)
                .text(cx),
            sample_text_2
        );

        workspace
            .go_back(workspace.active_pane().downgrade(), window, cx)
            .detach_and_log_err(cx);

        second_item_id
    });

    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, _, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating back from the 2nd buffer");
        assert_eq!(
            active_item.item_id(),
            multibuffer_item_id,
            "Should navigate back from the 2nd buffer to the multi buffer"
        );
        assert_eq!(active_item.buffer_kind(cx), ItemBufferKind::Multibuffer);
    });

    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(
            SelectionEffects::scroll(Autoscroll::Next),
            window,
            cx,
            |s| s.select_ranges(Some(MultiBufferOffset(70)..MultiBufferOffset(70))),
        );
        editor.open_excerpts(&OpenExcerpts, window, cx);
    });
    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, window, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating into the 3rd buffer");
        let third_item_id = active_item.item_id();
        assert_ne!(
            third_item_id, multibuffer_item_id,
            "Should navigate into the 3rd buffer and activate it"
        );
        assert_ne!(third_item_id, first_item_id);
        assert_ne!(third_item_id, second_item_id);
        assert_eq!(
            active_item.buffer_kind(cx),
            ItemBufferKind::Singleton,
            "New active item should be a singleton buffer"
        );
        assert_eq!(
            active_item
                .act_as::<Editor>(cx)
                .expect("should have navigated into an editor")
                .read(cx)
                .text(cx),
            sample_text_3
        );

        workspace
            .go_back(workspace.active_pane().downgrade(), window, cx)
            .detach_and_log_err(cx);
    });

    cx.executor().run_until_parked();
    workspace.update_in(cx, |workspace, _, cx| {
        let active_item = workspace
            .active_item(cx)
            .expect("should have an active item after navigating back from the 3rd buffer");
        assert_eq!(
            active_item.item_id(),
            multibuffer_item_id,
            "Should navigate back from the 3rd buffer to the multi buffer"
        );
        assert_eq!(active_item.buffer_kind(cx), ItemBufferKind::Multibuffer);
    });
}

#[gpui::test]
async fn test_toggle_selected_diff_hunks(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod;

        const A: u32 = 42;

        fn main() {
            println!("hello");

            println!("world");
        }
        "#
    .unindent();

    cx.set_state(
        &r#"
        use some::modified;

        ˇ
        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_hunk(&GoToHunk, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
          use some::modified;


          fn main() {
        -     println!("hello");
        + ˇ    println!("hello there");

              println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        for _ in 0..2 {
            editor.go_to_next_hunk(&GoToHunk, window, cx);
            editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
        }
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - use some::mod;
        + ˇuse some::modified;


          fn main() {
        -     println!("hello");
        +     println!("hello there");

        +     println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_hunk(&GoToHunk, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - use some::mod;
        + use some::modified;

        - const A: u32 = 42;
          ˇ
          fn main() {
        -     println!("hello");
        +     println!("hello there");

        +     println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.cancel(&Cancel, window, cx);
    });

    cx.assert_state_with_diff(
        r#"
          use some::modified;

          ˇ
          fn main() {
              println!("hello there");

              println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_diff_base_change_with_expanded_diff_hunks(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 42;

        fn main() {
            println!("hello");

            println!("world");
        }
        "#
    .unindent();

    cx.set_state(
        &r#"
        use some::mod2;

        const A: u32 = 42;
        const C: u32 = 42;

        fn main(ˇ) {
            //println!("hello");

            println!("world");
            //
            //
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - use some::mod1;
          use some::mod2;

          const A: u32 = 42;
        - const B: u32 = 42;
          const C: u32 = 42;

          fn main(ˇ) {
        -     println!("hello");
        +     //println!("hello");

              println!("world");
        +     //
        +     //
          }
        "#
        .unindent(),
    );

    cx.set_head_text("new diff base!");
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - new diff base!
        + use some::mod2;
        +
        + const A: u32 = 42;
        + const C: u32 = 42;
        +
        + fn main(ˇ) {
        +     //println!("hello");
        +
        +     println!("world");
        +     //
        +     //
        + }
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_toggle_diff_expand_in_multi_buffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let file_1_old = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj";
    let file_1_new = "aaa\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj";
    let file_2_old = "lll\nmmm\nnnn\nooo\nppp\nqqq\nrrr\nsss\nttt\nuuu";
    let file_2_new = "lll\nmmm\nNNN\nooo\nppp\nqqq\nrrr\nsss\nttt\nuuu";
    let file_3_old = "111\n222\n333\n444\n555\n777\n888\n999\n000\n!!!";
    let file_3_new = "111\n222\n333\n444\n555\n666\n777\n888\n999\n000\n!!!";

    let buffer_1 = cx.new(|cx| Buffer::local(file_1_new.to_string(), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(file_2_new.to_string(), cx));
    let buffer_3 = cx.new(|cx| Buffer::local(file_3_new.to_string(), cx));

    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 3),
                Point::new(5, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(10, 3),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 3),
                Point::new(5, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(10, 3),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 3),
                Point::new(5, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(10, 3),
            ],
            0,
            cx,
        );
        assert_eq!(multibuffer.read(cx).excerpts().count(), 9);
        multibuffer
    });

    let editor =
        cx.add_window(|window, cx| Editor::new(EditorMode::full(), multi_buffer, None, window, cx));
    editor
        .update(cx, |editor, _window, cx| {
            for (buffer, diff_base) in [
                (buffer_1.clone(), file_1_old),
                (buffer_2.clone(), file_2_old),
                (buffer_3.clone(), file_3_old),
            ] {
                let diff = cx.new(|cx| {
                    BufferDiff::new_with_base_text(diff_base, &buffer.read(cx).text_snapshot(), cx)
                });
                editor
                    .buffer
                    .update(cx, |buffer, cx| buffer.add_diff(diff, cx));
            }
        })
        .unwrap();

    let mut cx = EditorTestContext::for_editor(editor, cx).await;
    cx.run_until_parked();

    cx.assert_editor_state(
        &"
            ˇaaa
            ccc
            ddd
            ggg
            hhh

            lll
            mmm
            NNN
            qqq
            rrr
            uuu
            111
            222
            333
            666
            777
            000
            !!!"
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    cx.executor().run_until_parked();

    cx.assert_state_with_diff(
        "
            «aaa
          - bbb
            ccc
            ddd
            ggg
            hhh

            lll
            mmm
          - nnn
          + NNN
            qqq
            rrr
            uuu
            111
            222
            333
          + 666
            777
            000
            !!!ˇ»"
            .unindent(),
    );
}

#[gpui::test]
async fn test_expand_diff_hunk_at_excerpt_boundary(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let base = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\n";
    let text = "aaa\nBBB\nBB2\nccc\nDDD\nEEE\nfff\nggg\nhhh\niii\n";

    let buffer = cx.new(|cx| Buffer::local(text.to_string(), cx));
    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::new(0, 0)..Point::new(1, 3),
                Point::new(4, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(9, 3),
            ],
            0,
            cx,
        );
        assert_eq!(multibuffer.read(cx).excerpts().count(), 3);
        multibuffer
    });

    let editor =
        cx.add_window(|window, cx| Editor::new(EditorMode::full(), multi_buffer, None, window, cx));
    editor
        .update(cx, |editor, _window, cx| {
            let diff = cx.new(|cx| {
                BufferDiff::new_with_base_text(base, &buffer.read(cx).text_snapshot(), cx)
            });
            editor
                .buffer
                .update(cx, |buffer, cx| buffer.add_diff(diff, cx))
        })
        .unwrap();

    let mut cx = EditorTestContext::for_editor(editor, cx).await;
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&Default::default(), window, cx)
    });
    cx.executor().run_until_parked();

    // When the start of a hunk coincides with the start of its excerpt,
    // the hunk is expanded. When the start of a hunk is earlier than
    // the start of its excerpt, the hunk is not expanded.
    cx.assert_state_with_diff(
        "
            ˇaaa
          - bbb
          + BBB
          - ddd
          - eee
          + DDD
          + EEE
            fff
            iii"
        .unindent(),
    );
}

#[gpui::test]
async fn test_edits_around_expanded_insertion_hunks(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;

        fn main() {
            println!("hello");

            println!("world");
        }
        "#
    .unindent();
    executor.run_until_parked();
    cx.set_state(
        &r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 42;
        ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| editor.handle_input("const D: u32 = 42;\n", window, cx));
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + const D: u32 = 42;
      + ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| editor.handle_input("const E: u32 = 42;\n", window, cx));
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + const D: u32 = 42;
      + const E: u32 = 42;
      + ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + const D: u32 = 42;
      + const E: u32 = 42;
        ˇ
        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_up(&MoveUp, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
        ˇ
        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.select_up_by_lines(&SelectUpByLines { lines: 5 }, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        ˇ
        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_toggling_adjacent_diff_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_head_text(indoc! { "
        one
        two
        three
        four
        five
        "
    });
    cx.set_state(indoc! { "
        one
        ˇthree
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
        one
      - two
        ˇthree
      - four
        five
    "}
        .to_string(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });

    cx.assert_state_with_diff(
        indoc! { "
        one
        ˇthree
        five
    "}
        .to_string(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_up(&MoveUp, window, cx);
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
        ˇone
      - two
        three
        five
    "}
        .to_string(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
        editor.move_down(&MoveDown, window, cx);
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
        one
      - two
        ˇthree
      - four
        five
    "}
        .to_string(),
    );

    cx.set_state(indoc! { "
        one
        ˇTWO
        three
        four
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });

    cx.assert_state_with_diff(
        indoc! { "
            one
          - two
          + ˇTWO
            three
            four
            five
        "}
        .to_string(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.move_up(&Default::default(), window, cx);
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
            one
            ˇTWO
            three
            four
            five
        "}
        .to_string(),
    );
}

#[gpui::test]
async fn test_toggling_adjacent_diff_hunks_2(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        lineA
        lineB
        lineC
        lineD
        "#
    .unindent();

    cx.set_state(
        &r#"
        ˇlineA1
        lineB
        lineD
        "#
        .unindent(),
    );
    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - lineA
        + ˇlineA1
          lineB
          lineD
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
        editor.move_right(&MoveRight, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - lineA
        + lineA1
          lˇineB
        - lineC
          lineD
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_edits_around_expanded_deletion_hunks(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
    "#
    .unindent();
    executor.run_until_parked();
    cx.set_state(
        &r#"
        use some::mod1;
        use some::mod2;

        ˇconst B: u32 = 42;
        const C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
        ˇconst B: u32 = 42;
        const C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
      - const B: u32 = 42;
        ˇconst C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
      - const B: u32 = 42;
      - const C: u32 = 42;
        ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("replacement", window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
      - const B: u32 = 42;
      - const C: u32 = 42;
      -
      + replacementˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_backspace_after_deletion_hunk(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let base_text = r#"
        one
        two
        three
        four
        five
    "#
    .unindent();
    executor.run_until_parked();
    cx.set_state(
        &r#"
        one
        two
        fˇour
        five
        "#
        .unindent(),
    );

    cx.set_head_text(&base_text);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
          one
          two
        - three
          fˇour
          five
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Backspace, window, cx);
        editor.backspace(&Backspace, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
          one
          two
        - threeˇ
        - four
        + our
          five
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_edit_after_expanded_modification_hunk(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 42;
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
    .unindent();

    cx.set_state(
        &r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 43ˇ
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
      - const C: u32 = 42;
      + const C: u32 = 43ˇ
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\nnew_line\n", window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
      - const C: u32 = 42;
      + const C: u32 = 43
      + new_line
      + ˇ
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
        .unindent(),
    );
}

#[gpui::test]
async fn test_stage_and_unstage_added_file_hunk(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_editor(|editor, _, cx| {
        editor.set_expand_all_diff_hunks(cx);
    });

    let working_copy = r#"
            ˇfn main() {
                println!("hello, world!");
            }
        "#
    .unindent();

    cx.set_state(&working_copy);
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
            + ˇfn main() {
            +     println!("hello, world!");
            + }
        "#
        .unindent(),
    );
    cx.assert_index_text(None);

    cx.update_editor(|editor, window, cx| {
        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    executor.run_until_parked();
    cx.assert_index_text(Some(&working_copy.replace("ˇ", "")));
    cx.assert_state_with_diff(
        r#"
            + ˇfn main() {
            +     println!("hello, world!");
            + }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    executor.run_until_parked();
    cx.assert_index_text(None);
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

#[gpui::test]
async fn test_indent_guide_single_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..3, vec![indent_guide(buffer_id, 1, 1, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_simple_block(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
            let b = 2;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..4, vec![indent_guide(buffer_id, 1, 2, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_nested(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..8,
        vec![
            indent_guide(buffer_id, 1, 6, 0),
            indent_guide(buffer_id, 3, 3, 1),
            indent_guide(buffer_id, 5, 5, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_tab(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
                let b = 2;
            let c = 3;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..5,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_continues_on_empty_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..5, vec![indent_guide(buffer_id, 1, 3, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_complex(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;

            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..11,
        vec![
            indent_guide(buffer_id, 1, 9, 0),
            indent_guide(buffer_id, 6, 6, 1),
            indent_guide(buffer_id, 8, 8, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_starts_off_screen(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;

            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        1..11,
        vec![
            indent_guide(buffer_id, 1, 9, 0),
            indent_guide(buffer_id, 6, 6, 1),
            indent_guide(buffer_id, 8, 8, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_ends_off_screen(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;

            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        1..10,
        vec![
            indent_guide(buffer_id, 1, 9, 0),
            indent_guide(buffer_id, 6, 6, 1),
            indent_guide(buffer_id, 8, 8, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_with_folds(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            if a {
                b(
                    c,
                    d,
                )
            } else {
                e(
                    f
                )
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..11,
        vec![
            indent_guide(buffer_id, 1, 10, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 7, 9, 1),
            indent_guide(buffer_id, 3, 4, 2),
            indent_guide(buffer_id, 8, 8, 2),
        ],
        None,
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| {
        editor.fold_at(MultiBufferRow(2), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
            fn main() {
                if a {
                    b(⋯)
                } else {
                    e(
                        f
                    )
                }
            }"
            .unindent()
        );
    });

    assert_indent_guides(
        0..11,
        vec![
            indent_guide(buffer_id, 1, 10, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 7, 9, 1),
            indent_guide(buffer_id, 8, 8, 2),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_without_brackets(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        block1
            block2
                block3
                    block4
            block2
        block1
        block1"
            .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        1..10,
        vec![
            indent_guide(buffer_id, 1, 4, 0),
            indent_guide(buffer_id, 2, 3, 1),
            indent_guide(buffer_id, 3, 3, 2),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_ends_before_empty_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        block1
            block2
                block3

        block1
        block1"
            .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..6,
        vec![
            indent_guide(buffer_id, 1, 2, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_ignored_only_whitespace_lines(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        function component() {
        \treturn (
        \t\t\t
        \t\t<div>
        \t\t\t<abc></abc>
        \t\t</div>
        \t)
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..8,
        vec![
            indent_guide(buffer_id, 1, 6, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 4, 4, 2),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_fallback_to_next_non_entirely_whitespace_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        function component() {
        \treturn (
        \t
        \t\t<div>
        \t\t\t<abc></abc>
        \t\t</div>
        \t)
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..8,
        vec![
            indent_guide(buffer_id, 1, 6, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 4, 4, 2),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_continuing_off_screen(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        block1



            block2
        "
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..1, vec![indent_guide(buffer_id, 1, 1, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_tabs(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        def a:
        \tb = 3
        \tif True:
        \t\tc = 4
        \t\td = 5
        \tprint(b)
        "
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..6,
        vec![
            indent_guide(buffer_id, 1, 5, 0),
            indent_guide(buffer_id, 3, 4, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_single_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    fn main() {
        let a = 1;
    }"
        .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(1, 0)])
        });
    });

    assert_indent_guides(
        0..3,
        vec![indent_guide(buffer_id, 1, 1, 0)],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_respect_indented_range(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    fn main() {
        if 1 == 2 {
            let a = 1;
        }
    }"
        .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(1, 0)])
        });
    });
    cx.run_until_parked();

    assert_indent_guides(
        0..4,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        Some(vec![1]),
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(2, 0)..Point::new(2, 0)])
        });
    });
    cx.run_until_parked();

    assert_indent_guides(
        0..4,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        Some(vec![1]),
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(3, 0)..Point::new(3, 0)])
        });
    });
    cx.run_until_parked();

    assert_indent_guides(
        0..4,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_empty_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    fn main() {
        let a = 1;

        let b = 2;
    }"
        .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(2, 0)..Point::new(2, 0)])
        });
    });

    assert_indent_guides(
        0..5,
        vec![indent_guide(buffer_id, 1, 3, 0)],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_non_matching_indent(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    def m:
        a = 1
        pass"
            .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(1, 0)])
        });
    });

    assert_indent_guides(
        0..3,
        vec![indent_guide(buffer_id, 1, 2, 0)],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_with_expanded_diff_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    let text = indoc! {
        "
        impl A {
            fn b() {
                0;
                3;
                5;
                6;
                7;
            }
        }
        "
    };
    let base_text = indoc! {
        "
        impl A {
            fn b() {
                0;
                1;
                2;
                3;
                4;
            }
            fn c() {
                5;
                6;
                7;
            }
        }
        "
    };

    cx.update_editor(|editor, window, cx| {
        editor.set_text(text, window, cx);

        editor.buffer().update(cx, |multibuffer, cx| {
            let buffer = multibuffer.as_singleton().unwrap();
            let diff = cx.new(|cx| {
                BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx)
            });

            multibuffer.set_all_diff_hunks_expanded(cx);
            multibuffer.add_diff(diff, cx);

            buffer.read(cx).remote_id()
        })
    });
    cx.run_until_parked();

    cx.assert_state_with_diff(
        indoc! { "
          impl A {
              fn b() {
                  0;
        -         1;
        -         2;
                  3;
        -         4;
        -     }
        -     fn c() {
                  5;
                  6;
                  7;
              }
          }
          ˇ"
        }
        .to_string(),
    );

    let mut actual_guides = cx.update_editor(|editor, window, cx| {
        editor
            .snapshot(window, cx)
            .buffer_snapshot()
            .indent_guides_in_range(Anchor::Min..Anchor::Max, false, cx)
            .map(|guide| (guide.start_row..=guide.end_row, guide.depth))
            .collect::<Vec<_>>()
    });
    actual_guides.sort_by_key(|item| (*item.0.start(), item.1));
    assert_eq!(
        actual_guides,
        vec![
            (MultiBufferRow(1)..=MultiBufferRow(12), 0),
            (MultiBufferRow(2)..=MultiBufferRow(6), 1),
            (MultiBufferRow(9)..=MultiBufferRow(11), 1),
        ]
    );
}

#[gpui::test]
async fn test_adjacent_diff_hunks(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        a
        b
        c
        "#
    .unindent();

    cx.set_state(
        &r#"
        ˇA
        b
        C
        "#
        .unindent(),
    );
    cx.set_head_text(&diff_base);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    let both_hunks_expanded = r#"
        - a
        + ˇA
          b
        - c
        + C
        "#
    .unindent();

    cx.assert_state_with_diff(both_hunks_expanded.clone());

    let hunk_ranges = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
        hunks
            .into_iter()
            .map(|hunk| {
                multibuffer_snapshot
                    .anchor_in_excerpt(hunk.buffer_range.start)
                    .unwrap()
                    ..multibuffer_snapshot
                        .anchor_in_excerpt(hunk.buffer_range.end)
                        .unwrap()
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(hunk_ranges.len(), 2);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    let second_hunk_expanded = r#"
          ˇA
          b
        - c
        + C
        "#
    .unindent();

    cx.assert_state_with_diff(second_hunk_expanded);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(both_hunks_expanded.clone());

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[1].clone(), cx);
    });
    executor.run_until_parked();

    let first_hunk_expanded = r#"
        - a
        + ˇA
          b
          C
        "#
    .unindent();

    cx.assert_state_with_diff(first_hunk_expanded);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[1].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(both_hunks_expanded);

    cx.set_state(
        &r#"
        ˇA
        b
        "#
        .unindent(),
    );
    cx.run_until_parked();

    // TODO this cursor position seems bad
    cx.assert_state_with_diff(
        r#"
        - ˇa
        + A
          b
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });

    cx.assert_state_with_diff(
        r#"
            - ˇa
            + A
              b
            - c
            "#
        .unindent(),
    );

    let hunk_ranges = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        let multibuffer_snapshot = snapshot.buffer_snapshot();
        hunks
            .into_iter()
            .map(|hunk| {
                multibuffer_snapshot
                    .anchor_in_excerpt(hunk.buffer_range.start)
                    .unwrap()
                    ..multibuffer_snapshot
                        .anchor_in_excerpt(hunk.buffer_range.end)
                        .unwrap()
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(hunk_ranges.len(), 2);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[1].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        - ˇa
        + A
          b
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_toggle_deletion_hunk_at_start_of_file(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        a
        b
        c
        "#
    .unindent();

    cx.set_state(
        &r#"
        ˇb
        c
        "#
        .unindent(),
    );
    cx.set_head_text(&diff_base);
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    let hunk_expanded = r#"
        - a
          ˇb
          c
        "#
    .unindent();

    cx.assert_state_with_diff(hunk_expanded.clone());

    let hunk_ranges = cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
        hunks
            .into_iter()
            .map(|hunk| {
                multibuffer_snapshot
                    .anchor_in_excerpt(hunk.buffer_range.start)
                    .unwrap()
                    ..multibuffer_snapshot
                        .anchor_in_excerpt(hunk.buffer_range.end)
                        .unwrap()
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(hunk_ranges.len(), 1);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    let hunk_collapsed = r#"
          ˇb
          c
        "#
    .unindent();

    cx.assert_state_with_diff(hunk_collapsed);

    cx.update_editor(|editor, _, cx| {
        editor.toggle_single_diff_hunk(hunk_ranges[0].clone(), cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(hunk_expanded);
}

#[gpui::test]
async fn test_select_smaller_syntax_node_after_diff_hunk_collapse(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(rust_lang()), cx));

    cx.set_state(
        &r#"
        fn main() {
            let x = ˇ1;
        }
        "#
        .unindent(),
    );

    let diff_base = r#"
        fn removed_one() {
            println!("this function was deleted");
        }

        fn removed_two() {
            println!("this function was also deleted");
        }

        fn main() {
            let x = 1;
        }
        "#
    .unindent();
    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.select_larger_syntax_node(&SelectLargerSyntaxNode, window, cx);
    });

    cx.update_editor(|editor, window, cx| {
        editor.collapse_all_diff_hunks(&CollapseAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.select_smaller_syntax_node(&SelectSmallerSyntaxNode, window, cx);
    });
}

#[gpui::test]
async fn test_expand_first_line_diff_hunk_keeps_deleted_lines_visible(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("ˇnew\nsecond\nthird\n");
    cx.set_head_text("old\nsecond\nthird\n");
    cx.update_editor(|editor, window, cx| {
        editor.scroll(gpui::Point { x: 0., y: 0. }, None, window, cx);
    });
    executor.run_until_parked();
    assert_eq!(cx.update_editor(|e, _, cx| e.scroll_position(cx)).y, 0.0);

    // Expanding a diff hunk at the first line inserts deleted lines above the first buffer line.
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        assert_eq!(hunks.len(), 1);
        let hunk_range = multibuffer_snapshot
            .anchor_in_excerpt(hunks[0].buffer_range.start)
            .unwrap()
            ..multibuffer_snapshot
                .anchor_in_excerpt(hunks[0].buffer_range.end)
                .unwrap();
        editor.toggle_single_diff_hunk(hunk_range, cx)
    });
    executor.run_until_parked();
    cx.assert_state_with_diff("- old\n+ ˇnew\n  second\n  third\n".to_string());

    // Keep the editor scrolled to the top so the full hunk remains visible.
    assert_eq!(cx.update_editor(|e, _, cx| e.scroll_position(cx)).y, 0.0);
}

#[gpui::test]
async fn test_display_diff_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/test"),
        json!({
            ".git": {},
            "file-1": "ONE\n",
            "file-2": "TWO\n",
            "file-3": "THREE\n",
        }),
    )
    .await;

    fs.set_head_for_repo(
        path!("/test/.git").as_ref(),
        &[
            ("file-1", "one\n".into()),
            ("file-2", "two\n".into()),
            ("file-3", "three\n".into()),
        ],
        "deadbeef",
    );

    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;
    let mut buffers = vec![];
    for i in 1..=3 {
        let buffer = project
            .update(cx, |project, cx| {
                let path = format!(path!("/test/file-{}"), i);
                project.open_local_buffer(path, cx)
            })
            .await
            .unwrap();
        buffers.push(buffer);
    }

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_all_diff_hunks_expanded(cx);
        for buffer in &buffers {
            let snapshot = buffer.read(cx).snapshot();
            multibuffer.set_excerpts_for_path(
                PathKey::with_sort_prefix(0, buffer.read(cx).file().unwrap().path().clone()),
                buffer.clone(),
                vec![Point::zero()..snapshot.max_point()],
                2,
                cx,
            );
        }
        multibuffer
    });

    let editor = cx.add_window(|window, cx| {
        Editor::new(EditorMode::full(), multibuffer, Some(project), window, cx)
    });
    cx.run_until_parked();

    let snapshot = editor
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();
    let hunks = snapshot
        .display_diff_hunks_for_rows(DisplayRow(0)..DisplayRow(u32::MAX), &Default::default())
        .map(|hunk| match hunk {
            DisplayDiffHunk::Unfolded {
                display_row_range, ..
            } => display_row_range,
            DisplayDiffHunk::Folded { .. } => unreachable!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        hunks,
        [
            DisplayRow(2)..DisplayRow(4),
            DisplayRow(7)..DisplayRow(9),
            DisplayRow(12)..DisplayRow(14),
        ]
    );
}

#[gpui::test]
async fn test_partially_staged_hunk(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_head_text(indoc! { "
        one
        two
        three
        four
        five
        "
    });
    cx.set_index_text(indoc! { "
        one
        two
        three
        four
        five
        "
    });
    cx.set_state(indoc! {"
        one
        TWO
        ˇTHREE
        FOUR
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_index_text(Some(indoc! {"
        one
        TWO
        THREE
        FOUR
        five
    "}));
    cx.set_state(indoc! { "
        one
        TWO
        ˇTHREE-HUNDRED
        FOUR
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let hunks = editor
            .diff_hunks_in_ranges(&[Anchor::Min..Anchor::Max], &snapshot.buffer_snapshot())
            .collect::<Vec<_>>();
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            hunks[0].status(),
            DiffHunkStatus {
                kind: DiffHunkStatusKind::Modified,
                secondary: DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk
            }
        );

        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_index_text(Some(indoc! {"
        one
        TWO
        THREE-HUNDRED
        FOUR
        five
    "}));
}

#[gpui::test]
fn test_crease_insertion_and_rendering(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaaaa\nbbbbbb\ncccccc\nddddddd\n", cx);
        build_editor(buffer, window, cx)
    });

    let render_args = Arc::new(Mutex::new(None));
    let snapshot = editor
        .update(cx, |editor, window, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let range =
                snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_after(Point::new(2, 6));

            struct RenderArgs {
                row: MultiBufferRow,
                folded: bool,
                callback: Arc<dyn Fn(bool, &mut Window, &mut App) + Send + Sync>,
            }

            let crease = Crease::inline(
                range,
                FoldPlaceholder::test(),
                {
                    let toggle_callback = render_args.clone();
                    move |row, folded, callback, _window, _cx| {
                        *toggle_callback.lock() = Some(RenderArgs {
                            row,
                            folded,
                            callback,
                        });
                        div()
                    }
                },
                |_row, _folded, _window, _cx| div(),
            );

            editor.insert_creases(Some(crease), cx);
            let snapshot = editor.snapshot(window, cx);
            let _div =
                snapshot.render_crease_toggle(MultiBufferRow(1), false, cx.entity(), window, cx);
            snapshot
        })
        .unwrap();

    let render_args = render_args.lock().take().unwrap();
    assert_eq!(render_args.row, MultiBufferRow(1));
    assert!(!render_args.folded);
    assert!(!snapshot.is_line_folded(MultiBufferRow(1)));

    cx.update_window(*editor, |_, window, cx| {
        (render_args.callback)(true, window, cx)
    })
    .unwrap();
    let snapshot = editor
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();
    assert!(snapshot.is_line_folded(MultiBufferRow(1)));

    cx.update_window(*editor, |_, window, cx| {
        (render_args.callback)(false, window, cx)
    })
    .unwrap();
    let snapshot = editor
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();
    assert!(!snapshot.is_line_folded(MultiBufferRow(1)));
}

#[gpui::test]
async fn test_input_text(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(
        &r#"ˇone
        two

        three
        fourˇ
        five

        siˇx"#
            .unindent(),
    );

    cx.dispatch_action(HandleInput(String::new()));
    cx.assert_editor_state(
        &r#"ˇone
        two

        three
        fourˇ
        five

        siˇx"#
            .unindent(),
    );

    cx.dispatch_action(HandleInput("AAAA".to_string()));
    cx.assert_editor_state(
        &r#"AAAAˇone
        two

        three
        fourAAAAˇ
        five

        siAAAAˇx"#
            .unindent(),
    );
}

#[gpui::test]
async fn test_scroll_cursor_center_top_bottom(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        r#"let foo = 1;
let foo = 2;
let foo = 3;
let fooˇ = 4;
let foo = 5;
let foo = 6;
let foo = 7;
let foo = 8;
let foo = 9;
let foo = 10;
let foo = 11;
let foo = 12;
let foo = 13;
let foo = 14;
let foo = 15;"#,
    );

    cx.update_editor(|e, window, cx| {
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Center,
            "Default next scroll direction is center",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Top,
            "After center, next scroll direction should be top",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Bottom,
            "After top, next scroll direction should be bottom",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Center,
            "After bottom, scrolling should start over",
        );

        e.scroll_cursor_center_top_bottom(&ScrollCursorCenterTopBottom, window, cx);
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Top,
            "Scrolling continues if retriggered fast enough"
        );
    });

    cx.executor()
        .advance_clock(SCROLL_CENTER_TOP_BOTTOM_DEBOUNCE_TIMEOUT + Duration::from_millis(200));
    cx.executor().run_until_parked();
    cx.update_editor(|e, _, _| {
        assert_eq!(
            e.next_scroll_position,
            NextScrollCursorCenterTopBottom::Center,
            "If scrolling is not triggered fast enough, it should reset"
        );
    });
}

#[gpui::test]
async fn test_goto_definition_with_find_all_references_fallback(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let set_up_lsp_handlers = |empty_go_to_definition: bool, cx: &mut EditorLspTestContext| {
        let go_to_definition = cx
            .lsp
            .set_request_handler::<lsp::request::GotoDefinition, _, _>(
                move |params, _| async move {
                    if empty_go_to_definition {
                        Ok(None)
                    } else {
                        Ok(Some(lsp::GotoDefinitionResponse::Scalar(lsp::Location {
                            uri: params.text_document_position_params.text_document.uri,
                            range: lsp::Range::new(
                                lsp::Position::new(4, 3),
                                lsp::Position::new(4, 6),
                            ),
                        })))
                    }
                },
            );
        let references = cx
            .lsp
            .set_request_handler::<lsp::request::References, _, _>(move |params, _| async move {
                Ok(Some(vec![lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(0, 8), lsp::Position::new(0, 11)),
                }]))
            });
        (go_to_definition, references)
    };

    cx.set_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }

        fn two() {}"#
            .unindent(),
    );
    set_up_lsp_handlers(false, &mut cx);
    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definition");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to definition from the GetDefinition response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = two();
        }

        fn «twoˇ»() {}"#
            .unindent(),
    );

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, test_editor_cx| {
        assert_eq!(
            editors.len(),
            1,
            "Initially, only one, test, editor should be open in the workspace"
        );
        assert_eq!(
            test_editor_cx.entity(),
            editors.last().expect("Asserted len is 1").clone()
        );
    });

    set_up_lsp_handlers(true, &mut cx);
    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to lookup references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references as a fallback after empty GoToDefinition response"
    );
    // We should not change the selections in the existing file,
    // if opening another milti buffer with the references
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = two();
        }

        fn «twoˇ»() {}"#
            .unindent(),
    );
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, test_editor_cx| {
        assert_eq!(
            editors.len(),
            2,
            "After falling back to references search, we open a new editor with the results"
        );
        let references_fallback_text = editors
            .into_iter()
            .find(|new_editor| *new_editor != test_editor_cx.entity())
            .expect("Should have one non-test editor now")
            .read(test_editor_cx)
            .text(test_editor_cx);
        assert_eq!(
            references_fallback_text, "fn one() {\n    let mut a = two();\n}",
            "Should use the range from the references response and not the GoToDefinition one"
        );
    });
}

#[gpui::test]
async fn test_goto_definition_no_fallback(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        let mut editor_settings = EditorSettings::get_global(cx).clone();
        editor_settings.go_to_definition_fallback = GoToDefinitionFallback::None;
        EditorSettings::override_global(editor_settings, cx);
    });
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;
    let original_state = r#"fn one() {
        let mut a = ˇtwo();
    }

    fn two() {}"#
        .unindent();
    cx.set_state(&original_state);

    let mut go_to_definition = cx
        .lsp
        .set_request_handler::<lsp::request::GotoDefinition, _, _>(
            move |_, _| async move { Ok(None) },
        );
    let _references = cx
        .lsp
        .set_request_handler::<lsp::request::References, _, _>(move |_, _| async move {
            panic!("Should not call for references with no go to definition fallback")
        });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to lookup references");
    go_to_definition
        .next()
        .await
        .expect("Should have called the go_to_definition handler");

    assert_eq!(
        navigated,
        Navigated::No,
        "Should have navigated to references as a fallback after empty GoToDefinition response"
    );
    cx.assert_editor_state(&original_state);
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            1,
            "After unsuccessful fallback, no other editor should have been opened"
        );
    });
}

#[gpui::test]
async fn test_goto_definition_close_ranges_open_singleton(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // File content: 10 lines with functions defined on lines 3, 5, and 7 (0-indexed).
    // With the default excerpt_context_lines of 2, ranges that are within
    // 2 * 2 = 4 rows of each other should be grouped into one excerpt.
    cx.set_state(
        &r#"fn caller() {
            let _ = ˇtarget();
        }
        fn target_a() {}

        fn target_b() {}

        fn target_c() {}
        "#
        .unindent(),
    );

    // Return two definitions that are close together (lines 3 and 5, gap of 2 rows)
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(move |url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Array(vec![
            lsp::Location {
                uri: url.clone(),
                range: lsp::Range::new(lsp::Position::new(3, 3), lsp::Position::new(3, 11)),
            },
            lsp::Location {
                uri: url,
                range: lsp::Range::new(lsp::Position::new(5, 3), lsp::Position::new(5, 11)),
            },
        ])))
    });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definitions");
    assert_eq!(navigated, Navigated::Yes);

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            1,
            "Close ranges should navigate in-place without opening a new editor"
        );
    });

    // Both target ranges should be selected
    cx.assert_editor_state(
        &r#"fn caller() {
            let _ = target();
        }
        fn «target_aˇ»() {}

        fn «target_bˇ»() {}

        fn target_c() {}
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_goto_definition_far_ranges_open_multibuffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // Create a file with definitions far apart (more than 2 * excerpt_context_lines rows).
    cx.set_state(
        &r#"fn caller() {
            let _ = ˇtarget();
        }
        fn target_a() {}















        fn target_b() {}
        "#
        .unindent(),
    );

    // Return two definitions that are far apart (lines 3 and 19, gap of 16 rows)
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(move |url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Array(vec![
            lsp::Location {
                uri: url.clone(),
                range: lsp::Range::new(lsp::Position::new(3, 3), lsp::Position::new(3, 11)),
            },
            lsp::Location {
                uri: url,
                range: lsp::Range::new(lsp::Position::new(19, 3), lsp::Position::new(19, 11)),
            },
        ])))
    });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definitions");
    assert_eq!(navigated, Navigated::Yes);

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, test_editor_cx| {
        assert_eq!(
            editors.len(),
            2,
            "Far apart ranges should open a new multibuffer editor"
        );
        let multibuffer_editor = editors
            .into_iter()
            .find(|editor| *editor != test_editor_cx.entity())
            .expect("Should have a multibuffer editor");
        let multibuffer_text = multibuffer_editor.read(test_editor_cx).text(test_editor_cx);
        assert!(
            multibuffer_text.contains("target_a"),
            "Multibuffer should contain the first definition"
        );
        assert!(
            multibuffer_text.contains("target_b"),
            "Multibuffer should contain the second definition"
        );
    });
}

#[gpui::test]
async fn test_goto_definition_contained_ranges(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // The LSP returns two single-line definitions on the same row where one
    // range contains the other. Both are on the same line so the
    // `fits_in_one_excerpt` check won't underflow, and the code reaches
    // `change_selections`.
    cx.set_state(
        &r#"fn caller() {
            let _ = ˇtarget();
        }
        fn target_outer() { fn target_inner() {} }
        "#
        .unindent(),
    );

    // Return two definitions on the same line: an outer range covering the
    // whole line and an inner range for just the inner function name.
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(move |url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Array(vec![
            // Inner range: just "target_inner" (cols 23..35)
            lsp::Location {
                uri: url.clone(),
                range: lsp::Range::new(lsp::Position::new(3, 23), lsp::Position::new(3, 35)),
            },
            // Outer range: the whole line (cols 0..48)
            lsp::Location {
                uri: url,
                range: lsp::Range::new(lsp::Position::new(3, 0), lsp::Position::new(3, 48)),
            },
        ])))
    });

    let navigated = cx
        .update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definitions");
    assert_eq!(navigated, Navigated::Yes);
}

#[gpui::test]
async fn test_goto_definition_preserve_scroll_strategy(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.go_to_definition_scroll_strategy = Some(GoToDefinitionScrollStrategy::Preserve);
        settings.vertical_scroll_margin = Some(0.0);
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let window = cx.window;
    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    cx.simulate_window_resize(window, size(px(1000.), 8. * line_height));

    // Build a buffer where `target` is defined on row 10 and called from
    // row 20, with the cursor placed on the call site.
    let buffer = indoc! { "
            // 0
            // 1
            // 2
            // 3
            // 4
            // 5
            // 6
            // 7
            // 8
            // 9
            fn target() // 10
            // 11
            // 12
            // 13
            // 14
            // 15
            // 16
            // 17
            // 18
            // 19
            fn caller() { ˇtarget(); } // 20
            // 21
            // 22
            // 23
            // 24
            // 25
            // 26
            // 27
            // 28
            // 29
            // 30
        "};

    // Mock the response from the LSP server when requesting to go to a
    // definition so as to always jump to the `target` function.
    cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(|url, _, _| async move {
        Ok(Some(lsp::GotoDefinitionResponse::Scalar(lsp::Location {
            uri: url.clone(),
            range: lsp::Range::new(lsp::Position::new(10, 3), lsp::Position::new(10, 9)),
        })))
    });

    let caller_row = 20.0;
    let target_row = 10.0;
    let offset = 1.5;
    let center_offset = cx.update_editor(|editor, _, _| {
        editor
            .visible_line_count()
            .map(|count| ((count - 1.0) / 2.0).floor())
            .expect("Visible line count should be available")
    });

    // When the cursor is visible inside the viewport, going to a definition
    // should preserve that same offset value.
    // In this case, with the cursor set at row 20 and the scroll position set
    // to 18.5 (20 - 1.5), when going to the definition of `target` in row 10,
    // the scroll position should end up at 8.5 (10 - 1.5), so as to preserve
    // that same offset of 1.5.
    cx.set_state(&buffer);
    cx.update_editor(|editor, window, cx| {
        editor.set_scroll_position(gpui::Point::new(0.0, caller_row - offset), window, cx);
    });
    cx.update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definition");
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0.0, target_row - offset),
        );
    });

    // In the case where the cursor ends up outside of the visible viewport, the
    // scroll position's offset should be ignored and the center of the viewport
    // should be used instead.
    // Since the cursor is jumping to row 10, the scroll position's y coordinate
    // should end up at 10 minus the offset from the center of the viewport.
    cx.set_state(&buffer);
    cx.update_editor(|editor, window, cx| {
        editor.set_scroll_position(gpui::Point::new(0.0, 0.0), window, cx);
        let snapshot = editor.display_snapshot(cx);
        let cursor_row = editor
            .selections
            .newest_display(&snapshot)
            .start
            .row()
            .as_f64();
        let visible_lines = editor
            .visible_line_count()
            .expect("Visible line count should be available");

        assert!(cursor_row >= visible_lines, "Cursor should be offscreen");
    });

    cx.update_editor(|editor, window, cx| editor.go_to_definition(&GoToDefinition, window, cx))
        .await
        .expect("Failed to navigate to definition");
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            gpui::Point::new(0.0, (target_row - center_offset).max(0.0)),
        );
    });
}

#[gpui::test]
async fn test_find_all_references_editor_reuse(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            references_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    cx.set_state(
        &r#"
        fn one() {
            let mut a = two();
        }

        fn ˇtwo() {}"#
            .unindent(),
    );
    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(move |params, _| async move {
            Ok(Some(vec![
                lsp::Location {
                    uri: params.text_document_position.text_document.uri.clone(),
                    range: lsp::Range::new(lsp::Position::new(0, 16), lsp::Position::new(0, 19)),
                },
                lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(4, 4), lsp::Position::new(4, 7)),
                },
            ]))
        });
    let navigated = cx
        .update_editor(|editor, window, cx| {
            editor.find_all_references(&FindAllReferences::default(), window, cx)
        })
        .unwrap()
        .await
        .expect("Failed to navigate to references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references from the FindAllReferences response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = two();
        }

        fn ˇtwo() {}"#
            .unindent(),
    );

    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(editors.len(), 2, "We should have opened a new multibuffer");
    });

    cx.set_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }

        fn two() {}"#
            .unindent(),
    );
    let navigated = cx
        .update_editor(|editor, window, cx| {
            editor.find_all_references(&FindAllReferences::default(), window, cx)
        })
        .unwrap()
        .await
        .expect("Failed to navigate to references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references from the FindAllReferences response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }

        fn two() {}"#
            .unindent(),
    );
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            2,
            "should have re-used the previous multibuffer"
        );
    });

    cx.set_state(
        &r#"fn one() {
            let mut a = ˇtwo();
        }
        fn three() {}
        fn two() {}"#
            .unindent(),
    );
    cx.lsp
        .set_request_handler::<lsp::request::References, _, _>(move |params, _| async move {
            Ok(Some(vec![
                lsp::Location {
                    uri: params.text_document_position.text_document.uri.clone(),
                    range: lsp::Range::new(lsp::Position::new(0, 16), lsp::Position::new(0, 19)),
                },
                lsp::Location {
                    uri: params.text_document_position.text_document.uri,
                    range: lsp::Range::new(lsp::Position::new(5, 4), lsp::Position::new(5, 7)),
                },
            ]))
        });
    let navigated = cx
        .update_editor(|editor, window, cx| {
            editor.find_all_references(&FindAllReferences::default(), window, cx)
        })
        .unwrap()
        .await
        .expect("Failed to navigate to references");
    assert_eq!(
        navigated,
        Navigated::Yes,
        "Should have navigated to references from the FindAllReferences response"
    );
    cx.assert_editor_state(
        &r#"fn one() {
                let mut a = ˇtwo();
            }
            fn three() {}
            fn two() {}"#
            .unindent(),
    );
    let editors = cx.update_workspace(|workspace, _, cx| {
        workspace.items_of_type::<Editor>(cx).collect::<Vec<_>>()
    });
    cx.update_editor(|_, _, _| {
        assert_eq!(
            editors.len(),
            3,
            "should have used a new multibuffer as offsets changed"
        );
    });
}
#[gpui::test]
async fn test_find_enclosing_node_with_task(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    let text = r#"
        #[cfg(test)]
        mod tests() {
            #[test]
            fn runnable_1() {
                let a = 1;
            }

            #[test]
            fn runnable_2() {
                let a = 1;
                let b = 2;
            }
        }
    "#
    .unindent();

    let fs = FakeFs::new(cx.executor());
    fs.insert_file("/file.rs", Default::default()).await;

    let project = Project::test(fs, ["/a".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        editor.runnables.insert(
            buffer.read(cx).remote_id(),
            3,
            buffer.read(cx).version(),
            RunnableTasks {
                templates: Vec::new(),
                offset: snapshot.anchor_before(MultiBufferOffset(43)),
                column: 0,
                extra_variables: HashMap::default(),
                context_range: BufferOffset(43)..BufferOffset(85),
            },
        );
        editor.runnables.insert(
            buffer.read(cx).remote_id(),
            8,
            buffer.read(cx).version(),
            RunnableTasks {
                templates: Vec::new(),
                offset: snapshot.anchor_before(MultiBufferOffset(86)),
                column: 0,
                extra_variables: HashMap::default(),
                context_range: BufferOffset(86)..BufferOffset(191),
            },
        );

        // Test finding task when cursor is inside function body
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(4, 5)..Point::new(4, 5)])
        });
        let (_, row, _) = editor.find_enclosing_node_task(cx).unwrap();
        assert_eq!(row, 3, "Should find task for cursor inside runnable_1");

        // Test finding task when cursor is on function name
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(8, 4)..Point::new(8, 4)])
        });
        let (_, row, _) = editor.find_enclosing_node_task(cx).unwrap();
        assert_eq!(row, 8, "Should find task when cursor is on function name");
    });
}

#[gpui::test]
async fn test_toggle_code_actions_build_tasks_context_error_notifies(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    struct FailingContextProvider;
    impl ContextProvider for FailingContextProvider {
        fn build_context(
            &self,
            _: &TaskVariables,
            _: ContextLocation<'_>,
            _: Option<HashMap<String, String>>,
            _: Arc<dyn LanguageToolchainStore>,
            _: &mut gpui::App,
        ) -> Task<anyhow::Result<TaskVariables>> {
            Task::ready(Err(anyhow::anyhow!("Task context provider failed")))
        }
    }

    let language = Arc::new(
        Arc::try_unwrap(rust_lang())
            .unwrap()
            .with_context_provider(Some(Arc::new(FailingContextProvider))),
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/a"), json!({ "main.rs": "fn main() {}" }))
        .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(language.clone());

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let mut cx = VisualTestContext::from_window(*window, cx);
    let workspace = window
        .read_with(&mut cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let worktree_id = workspace.update_in(&mut cx, |workspace, _, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let editor = workspace
        .update_in(&mut cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    editor.update_in(&mut cx, |editor, window, cx| {
        let buffer = editor.buffer().read(cx).as_singleton().unwrap();
        buffer.update(cx, |buffer, cx| {
            buffer.set_language(Some(language.clone()), cx)
        });

        let snapshot = editor.buffer().read(cx).snapshot(cx);
        editor.runnables.insert(
            buffer.read(cx).remote_id(),
            0,
            buffer.read(cx).version(),
            RunnableTasks {
                templates: Vec::new(),
                offset: snapshot.anchor_before(MultiBufferOffset(0)),
                column: 0,
                extra_variables: HashMap::default(),
                context_range: BufferOffset(0)..BufferOffset(0),
            },
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 0)])
        });

        editor.toggle_code_actions(
            &ToggleCodeActions {
                deployed_from: None,
                quick_launch: false,
            },
            window,
            cx,
        );
    });

    cx.run_until_parked();

    workspace.update_in(&mut cx, |workspace, _, _| {
        assert!(!workspace.notification_ids().is_empty());
    });
}

#[gpui::test]
async fn test_folding_buffers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text_1 = "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj".to_string();
    let sample_text_2 = "llll\nmmmm\nnnnn\noooo\npppp\nqqqq\nrrrr\nssss\ntttt\nuuuu".to_string();
    let sample_text_3 = "vvvv\nwwww\nxxxx\nyyyy\nzzzz\n1111\n2222\n3333\n4444\n5555".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "first.rs": sample_text_1,
            "second.rs": sample_text_2,
            "third.rs": sample_text_3,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("first.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("second.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_3 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("third.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(10, 4),
            ],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(10, 4),
            ],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 0),
                Point::new(5, 0)..Point::new(6, 0),
                Point::new(9, 0)..Point::new(10, 4),
            ],
            0,
            cx,
        );
        multi_buffer
    });
    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer.clone(),
            Some(project.clone()),
            window,
            cx,
        )
    });

    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\naaaa\nbbbb\ncccc\n\nffff\ngggg\n\njjjj\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_1.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
        "After folding the first buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_2.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
        "After folding the second buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n",
        "After folding the third buffer, its text should not be displayed"
    );

    // Emulate selection inside the fold logic, that should work
    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        editor
            .snapshot(window, cx)
            .next_line_boundary(Point::new(0, 4));
    });

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_2.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n",
        "After unfolding the second buffer, its text should be displayed"
    );

    // Typing inside of buffer 1 causes that buffer to be unfolded.
    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            multi_buffer
                .read(cx)
                .snapshot(cx)
                .text_for_range(Point::new(1, 0)..Point::new(1, 4))
                .collect::<String>(),
            "bbbb"
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges(vec![Point::new(1, 0)..Point::new(1, 0)]);
        });
        editor.handle_input("B", window, cx);
    });

    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\naaaa\nBbbbb\ncccc\n\nffff\ngggg\n\njjjj\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n",
        "After unfolding the first buffer, its and 2nd buffer's text should be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\naaaa\nBbbbb\ncccc\n\nffff\ngggg\n\njjjj\n\n\nllll\nmmmm\nnnnn\n\nqqqq\nrrrr\n\nuuuu\n\n\nvvvv\nwwww\nxxxx\n\n1111\n2222\n\n5555",
        "After unfolding the all buffers, all original text should be displayed"
    );
}

#[gpui::test]
async fn test_folded_buffers_cleared_on_excerpts_removed(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "file_a.txt": "File A\nFile A\nFile A",
            "file_b.txt": "File B\nFile B\nFile B",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let buffer_a = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("file_a.txt")), cx)
        })
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("file_b.txt")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        let range_a = Point::new(0, 0)..Point::new(2, 4);
        let range_b = Point::new(0, 0)..Point::new(2, 4);

        multi_buffer.set_excerpts_for_path(PathKey::sorted(0), buffer_a.clone(), [range_a], 0, cx);
        multi_buffer.set_excerpts_for_path(PathKey::sorted(1), buffer_b.clone(), [range_b], 0, cx);
        multi_buffer
    });

    let editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer.clone(),
            Some(project.clone()),
            window,
            cx,
        )
    });

    editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_a.read(cx).remote_id(), cx);
    });
    assert!(editor.update(cx, |editor, cx| editor.has_any_buffer_folded(cx)));

    // When the excerpts for `buffer_a` are removed, a
    // `multi_buffer::Event::ExcerptsRemoved` event is emitted, which should be
    // picked up by the editor and update the display map accordingly.
    multi_buffer.update(cx, |multi_buffer, cx| {
        multi_buffer.remove_excerpts(PathKey::sorted(0), cx)
    });
    assert!(!editor.update(cx, |editor, cx| editor.has_any_buffer_folded(cx)));
}

#[gpui::test]
async fn test_folding_buffers_with_one_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text_1 = "1111\n2222\n3333".to_string();
    let sample_text_2 = "4444\n5555\n6666".to_string();
    let sample_text_3 = "7777\n8888\n9999".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "first.rs": sample_text_1,
            "second.rs": sample_text_2,
            "third.rs": sample_text_3,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("first.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("second.rs")), cx)
        })
        .await
        .unwrap();
    let buffer_3 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("third.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(3, 0)],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(3, 0)],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [Point::new(0, 0)..Point::new(3, 0)],
            0,
            cx,
        );
        multi_buffer
    });

    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    let full_text = "\n\n1111\n2222\n3333\n\n\n4444\n5555\n6666\n\n\n7777\n8888\n9999";
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        full_text,
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_1.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n4444\n5555\n6666\n\n\n7777\n8888\n9999",
        "After folding the first buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_2.read(cx).remote_id(), cx)
    });

    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n\n7777\n8888\n9999",
        "After folding the second buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.fold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n\n",
        "After folding the third buffer, its text should not be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_2.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n\n\n4444\n5555\n6666\n\n",
        "After unfolding the second buffer, its text should be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_1.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        "\n\n1111\n2222\n3333\n\n\n4444\n5555\n6666\n\n",
        "After unfolding the first buffer, its text should be displayed"
    );

    multi_buffer_editor.update(cx, |editor, cx| {
        editor.unfold_buffer(buffer_3.read(cx).remote_id(), cx)
    });
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        full_text,
        "After unfolding all buffers, all original text should be displayed"
    );
}

#[gpui::test]
async fn test_folding_buffer_when_multibuffer_has_only_one_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text = "aaaa\nbbbb\ncccc\ndddd\neeee\nffff\ngggg\nhhhh\niiii\njjjj".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let buffer_1 = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)
                ..Point::new(
                    sample_text.chars().filter(|&c| c == '\n').count() as u32 + 1,
                    0,
                )],
            0,
            cx,
        );
        multi_buffer
    });
    let multi_buffer_editor = cx.new_window_entity(|window, cx| {
        Editor::new(
            EditorMode::full(),
            multi_buffer,
            Some(project.clone()),
            window,
            cx,
        )
    });

    let selection_range = Point::new(1, 0)..Point::new(2, 0);
    multi_buffer_editor.update_in(cx, |editor, window, cx| {
        let multi_buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
        let highlight_range = selection_range.clone().to_anchors(&multi_buffer_snapshot);
        editor.highlight_text(
            HighlightKey::Editor,
            vec![highlight_range.clone()],
            HighlightStyle::color(Hsla::green()),
            cx,
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(highlight_range))
        });
    });

    let full_text = format!("\n\n{sample_text}");
    assert_eq!(
        multi_buffer_editor.update(cx, |editor, cx| editor.display_text(cx)),
        full_text,
    );
}

#[gpui::test]
async fn test_multi_buffer_navigation_with_folded_buffers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        let default_key_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-linux.json",
            cx,
        )
        .unwrap();
        cx.bind_keys(default_key_bindings);
    });

    let (editor, cx) = cx.add_window_view(|window, cx| {
        let multi_buffer = MultiBuffer::build_multi(
            [
                ("a0\nb0\nc0\nd0\ne0\n", vec![Point::row_range(0..2)]),
                ("a1\nb1\nc1\nd1\ne1\n", vec![Point::row_range(0..2)]),
                ("a2\nb2\nc2\nd2\ne2\n", vec![Point::row_range(0..2)]),
                ("a3\nb3\nc3\nd3\ne3\n", vec![Point::row_range(0..2)]),
            ],
            cx,
        );
        let mut editor = Editor::new(EditorMode::full(), multi_buffer.clone(), None, window, cx);

        let buffer_ids = multi_buffer
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<Vec<_>>();
        // fold all but the second buffer, so that we test navigating between two
        // adjacent folded buffers, as well as folded buffers at the start and
        // end the multibuffer
        editor.fold_buffer(buffer_ids[0], cx);
        editor.fold_buffer(buffer_ids[2], cx);
        editor.fold_buffer(buffer_ids[3], cx);

        editor
    });
    cx.simulate_resize(size(px(1000.), px(1000.)));

    let mut cx = EditorTestContext::for_editor_in(editor.clone(), cx).await;
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        a1
        b1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇa1
        b1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        ˇb1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        ˇ[EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("down");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    for _ in 0..5 {
        cx.simulate_keystroke("down");
        cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            [FOLDED]
            [EXCERPT]
            a1
            b1
            [EXCERPT]
            [FOLDED]
            [EXCERPT]
            ˇ[FOLDED]
            "
        });
    }

    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        [EXCERPT]
        ˇ[FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        b1
        ˇ[EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        a1
        ˇb1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    cx.simulate_keystroke("up");
    cx.assert_excerpts_with_selections(indoc! {"
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        ˇa1
        b1
        [EXCERPT]
        [FOLDED]
        [EXCERPT]
        [FOLDED]
        "
    });
    for _ in 0..5 {
        cx.simulate_keystroke("up");
        cx.assert_excerpts_with_selections(indoc! {"
            [EXCERPT]
            ˇ[FOLDED]
            [EXCERPT]
            a1
            b1
            [EXCERPT]
            [FOLDED]
            [EXCERPT]
            [FOLDED]
            "
        });
    }
}

#[gpui::test]
async fn test_edit_prediction_text(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Simple insertion
    assert_highlighted_edits(
        "Hello, world!",
        vec![(Point::new(0, 6)..Point::new(0, 6), " beautiful".into())],
        true,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "Hello, beautiful world!");
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 6..16);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;

    // Replacement
    assert_highlighted_edits(
        "This is a test.",
        vec![(Point::new(0, 0)..Point::new(0, 4), "That".into())],
        false,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "That is a test.");
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 0..4);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;

    // Multiple edits
    assert_highlighted_edits(
        "Hello, world!",
        vec![
            (Point::new(0, 0)..Point::new(0, 5), "Greetings".into()),
            (Point::new(0, 12)..Point::new(0, 12), " and universe".into()),
        ],
        false,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "Greetings, world and universe!");
            assert_eq!(highlighted_edits.highlights.len(), 2);
            assert_eq!(highlighted_edits.highlights[0].0, 0..9);
            assert_eq!(highlighted_edits.highlights[1].0, 16..29);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
            assert_eq!(
                highlighted_edits.highlights[1].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;

    // Multiple lines with edits
    assert_highlighted_edits(
        "First line\nSecond line\nThird line\nFourth line",
        vec![
            (Point::new(1, 7)..Point::new(1, 11), "modified".to_string()),
            (
                Point::new(2, 0)..Point::new(2, 10),
                "New third line".to_string(),
            ),
            (Point::new(3, 6)..Point::new(3, 6), " updated".to_string()),
        ],
        false,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(
                highlighted_edits.text,
                "Second modified\nNew third line\nFourth updated line"
            );
            assert_eq!(highlighted_edits.highlights.len(), 3);
            assert_eq!(highlighted_edits.highlights[0].0, 7..15); // "modified"
            assert_eq!(highlighted_edits.highlights[1].0, 16..30); // "New third line"
            assert_eq!(highlighted_edits.highlights[2].0, 37..45); // " updated"
            for highlight in &highlighted_edits.highlights {
                assert_eq!(
                    highlight.1.background_color,
                    Some(cx.theme().status().created_background)
                );
            }
        },
    )
    .await;
}

#[gpui::test]
async fn test_edit_prediction_text_with_deletions(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Deletion
    assert_highlighted_edits(
        "Hello, world!",
        vec![(Point::new(0, 5)..Point::new(0, 11), "".to_string())],
        true,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "Hello, world!");
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 5..11);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().deleted_background)
            );
        },
    )
    .await;

    // Insertion
    assert_highlighted_edits(
        "Hello, world!",
        vec![(Point::new(0, 6)..Point::new(0, 6), " digital".to_string())],
        true,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 6..14);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;
}

async fn assert_highlighted_edits(
    text: &str,
    edits: Vec<(Range<Point>, String)>,
    include_deletions: bool,
    cx: &mut TestAppContext,
    assertion_fn: &dyn Fn(HighlightedText, &App),
) {
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(text, cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let (buffer, snapshot) = window
        .update(cx, |editor, _window, cx| {
            (
                editor.buffer().clone(),
                editor.buffer().read(cx).snapshot(cx),
            )
        })
        .unwrap();

    let edits = edits
        .into_iter()
        .map(|(range, edit)| {
            (
                snapshot.anchor_after(range.start)..snapshot.anchor_before(range.end),
                edit,
            )
        })
        .collect::<Vec<_>>();

    let text_anchor_edits = edits
        .clone()
        .into_iter()
        .map(|(range, edit)| {
            (
                range.start.expect_text_anchor()..range.end.expect_text_anchor(),
                edit.into(),
            )
        })
        .collect::<Vec<_>>();

    let edit_preview = window
        .update(cx, |_, _window, cx| {
            buffer
                .read(cx)
                .as_singleton()
                .unwrap()
                .read(cx)
                .preview_edits(text_anchor_edits.into(), cx)
        })
        .unwrap()
        .await;

    cx.update(|_window, cx| {
        let highlighted_edits = edit_prediction_edit_text(
            snapshot.as_singleton().unwrap(),
            &edits,
            &edit_preview,
            include_deletions,
            &snapshot,
            cx,
        );
        assertion_fn(highlighted_edits, cx)
    });
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

#[gpui::test]
async fn test_breakpoint_toggling(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text = "First line\nSecond line\nThird line\nFourth line".to_string();
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
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

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    let project_path = editor.update(cx, |editor, cx| editor.active_project_path(cx).unwrap());
    let abs_path = project.read_with(cx, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

    // assert we can add breakpoint on the first line
    editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_standard()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_beginning(&MoveToBeginning, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![(3, Breakpoint::new_standard())],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(0, breakpoints.len());
    assert_breakpoint(&breakpoints, &abs_path, vec![]);
}

#[gpui::test]
async fn test_breakpoint_after_save_as_existing_path(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());

    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let first_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (first_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(first_buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    first_editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let replacement_buffer = project.update(cx, |project, cx| {
        project.create_local_buffer("Alpha\nBeta\nGamma", None, true, cx)
    });
    project
        .update(cx, |project, cx| {
            project.save_buffer_as(
                replacement_buffer.clone(),
                ProjectPath {
                    worktree_id,
                    path: rel_path("main.rs").into(),
                },
                cx,
            )
        })
        .await
        .unwrap();

    let (replacement_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(replacement_buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    replacement_editor.update_in(cx, |editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let project_path =
        first_editor.update(cx, |editor, cx| editor.active_project_path(cx).unwrap());
    let abs_path = project.read_with(cx, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

    let breakpoints = first_editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .source_breakpoints_from_path(&abs_path, cx)
    });

    assert_eq!(
        vec![0, 1],
        breakpoints
            .into_iter()
            .map(|breakpoint| breakpoint.row)
            .collect::<Vec<_>>()
    );
}

#[gpui::test]
async fn test_log_breakpoint_editing(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text = "First line\nSecond line\nThird line\nFourth line".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
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

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    let project_path = editor.update(cx, |editor, cx| editor.active_project_path(cx).unwrap());
    let abs_path = project.read_with(cx, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "hello world", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![(0, Breakpoint::new_log("hello world"))],
    );

    // Removing a log message from a log breakpoint should remove it
    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(&breakpoints, &abs_path, vec![]);

    editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        // Not adding a log message to a standard breakpoint shouldn't remove it
        add_log_breakpoint_at_cursor(editor, "", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_standard()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "hello world", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_log("hello world")),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "hello Earth!!", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_log("hello Earth!!")),
        ],
    );
}

/// This also tests that Editor::breakpoint_at_cursor_head is working properly
/// we had some issues where we wouldn't find a breakpoint at Point {row: 0, col: 0}
/// or when breakpoints were placed out of order. This tests for a regression too
#[gpui::test]
async fn test_breakpoint_enabling_and_disabling(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text = "First line\nSecond line\nThird line\nFourth line".to_string();
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
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

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    let project_path = editor.update(cx, |editor, cx| editor.active_project_path(cx).unwrap());
    let abs_path = project.read_with(cx, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

    // assert we can add breakpoint on the first line
    editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (2, Breakpoint::new_standard()),
            (3, Breakpoint::new_standard()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_beginning(&MoveToBeginning, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
        // Disabling a breakpoint that doesn't exist should do nothing
        editor.move_up(&MoveUp, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    let disable_breakpoint = {
        let mut bp = Breakpoint::new_standard();
        bp.state = BreakpointState::Disabled;
        bp
    };

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, disable_breakpoint.clone()),
            (2, Breakpoint::new_standard()),
            (3, disable_breakpoint.clone()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_beginning(&MoveToBeginning, window, cx);
        editor.enable_breakpoint(&actions::EnableBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.enable_breakpoint(&actions::EnableBreakpoint, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (2, disable_breakpoint),
            (3, Breakpoint::new_standard()),
        ],
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

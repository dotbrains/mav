use super::*;

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

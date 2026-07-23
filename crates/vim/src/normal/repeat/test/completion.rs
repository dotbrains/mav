use editor::test::editor_lsp_test_context::EditorLspTestContext;
use futures::StreamExt;
use indoc::indoc;

use gpui::EntityInputHandler;

use crate::{
    VimGlobals,
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
#[gpui::test]
async fn test_repeat_ime(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("hˇllo", Mode::Normal);
    cx.simulate_keystrokes("i");

    // simulate brazilian input for ä.
    cx.update_editor(|editor, window, cx| {
        editor.replace_and_mark_text_in_range(None, "\"", Some(1..1), window, cx);
        editor.replace_text_in_range(None, "ä", window, cx);
    });
    cx.simulate_keystrokes("escape");
    cx.assert_state("hˇällo", Mode::Normal);
    cx.simulate_keystrokes(".");
    cx.assert_state("hˇäällo", Mode::Normal);
}

#[gpui::test]
async fn test_repeat_completion(cx: &mut gpui::TestAppContext) {
    VimTestContext::init(cx);
    let cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;
    let mut cx = VimTestContext::new_with_lsp(cx, true);

    cx.set_state(
        indoc! {"
            onˇe
            two
            three
        "},
        Mode::Normal,
    );

    let mut request =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, params, _| async move {
            let position = params.text_document_position.position;
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "first".to_string(),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::new(position, position),
                        new_text: "first".to_string(),
                    })),
                    ..Default::default()
                },
                lsp::CompletionItem {
                    label: "second".to_string(),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::new(position, position),
                        new_text: "second".to_string(),
                    })),
                    ..Default::default()
                },
            ])))
        });
    cx.simulate_keystrokes("a .");
    request.next().await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.simulate_keystrokes("down enter ! escape");

    cx.assert_state(
        indoc! {"
                one.secondˇ!
                two
                three
            "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("j .");
    cx.assert_state(
        indoc! {"
                one.second!
                two.secondˇ!
                three
            "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_repeat_completion_unicode_bug(cx: &mut gpui::TestAppContext) {
    VimTestContext::init(cx);
    let cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;
    let mut cx = VimTestContext::new_with_lsp(cx, true);

    cx.set_state(
        indoc! {"
                ĩлˇк
                ĩлк
            "},
        Mode::Normal,
    );

    let mut request =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, params, _| async move {
            let position = params.text_document_position.position;
            let mut to_the_left = position;
            to_the_left.character -= 2;
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "oops".to_string(),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::new(to_the_left, position),
                        new_text: "к!".to_string(),
                    })),
                    ..Default::default()
                },
            ])))
        });
    cx.simulate_keystrokes("i .");
    request.next().await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.simulate_keystrokes("enter escape");
    cx.assert_state(
        indoc! {"
                ĩкˇ!к
                ĩлк
            "},
        Mode::Normal,
    );
}

use crate::{
    state::{Mode, Register},
    test::{NeovimBackedTestContext, VimTestContext},
};
use gpui::ClipboardItem;
use indoc::indoc;
use language::{LanguageName, language_settings::LanguageSettingsContent};
use settings::{SettingsStore, UseSystemClipboard};
#[gpui::test]
async fn test_multicursor_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_system_clipboard = Some(UseSystemClipboard::Never)
        });
    });

    cx.set_state(
        indoc! {"
               ˇfish one
               fish two
               fish red
               fish blue
                "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("4 g l w escape d i w 0 shift-p");
    cx.assert_state(
        indoc! {"
               onˇefish•
               twˇofish•
               reˇdfish•
               bluˇefish•
                "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_replace_with_register(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
                   ˇfish one
                   two three
                   "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y i w");
    cx.simulate_keystrokes("w");
    cx.simulate_keystrokes("g shift-r i w");
    cx.assert_state(
        indoc! {"
                fish fisˇh
                two three
                "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("j b g shift-r e");
    cx.assert_state(
        indoc! {"
            fish fish
            two fisˇh
            "},
        Mode::Normal,
    );
    let clipboard: Register = cx.read_from_clipboard().unwrap().into();
    assert_eq!(clipboard.text, "fish");

    cx.set_state(
        indoc! {"
                   ˇfish one
                   two three
                   "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y i w");
    cx.simulate_keystrokes("w");
    cx.simulate_keystrokes("v i w g shift-r");
    cx.assert_state(
        indoc! {"
                fish fisˇh
                two three
                "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("g shift-r r");
    cx.assert_state(
        indoc! {"
                fisˇh
                two three
                "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("j w g shift-r $");
    cx.assert_state(
        indoc! {"
                fish
                two fisˇh
            "},
        Mode::Normal,
    );
    let clipboard: Register = cx.read_from_clipboard().unwrap().into();
    assert_eq!(clipboard.text, "fish");
}

#[gpui::test]
async fn test_replace_with_register_dot_repeat(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
                   ˇfish one
                   two three
                   "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("y i w");
    cx.simulate_keystrokes("w");
    cx.simulate_keystrokes("g shift-r i w");
    cx.assert_state(
        indoc! {"
                fish fisˇh
                two three
                "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("j .");
    cx.assert_state(
        indoc! {"
                fish fish
                two fisˇh
                "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_paste_entire_line_from_editor_copy(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
                ˇline one
                line two
                line three"},
        Mode::Normal,
    );

    // Simulate what the editor's do_copy produces for two entire-line selections:
    // entire-line selections are NOT separated by an extra newline in the clipboard text.
    let clipboard_text = "line one\nline two\n".to_string();
    let clipboard_selections = vec![
        editor::ClipboardSelection {
            len: "line one\n".len(),
            is_entire_line: true,
            first_line_indent: 0,
            file_path: None,
            line_range: None,
        },
        editor::ClipboardSelection {
            len: "line two\n".len(),
            is_entire_line: true,
            first_line_indent: 0,
            file_path: None,
            line_range: None,
        },
    ];
    cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
        clipboard_text,
        clipboard_selections,
    ));

    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
                line one
                ˇline one
                line two
                line two
                line three"},
        Mode::Normal,
    );
}

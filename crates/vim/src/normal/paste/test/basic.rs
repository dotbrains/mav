
use crate::{
    state::{Mode, Register},
    test::{NeovimBackedTestContext, VimTestContext},
};
use gpui::ClipboardItem;
use indoc::indoc;
use language::{LanguageName, language_settings::LanguageSettingsContent};
use settings::{SettingsStore, UseSystemClipboard};
#[gpui::test]
async fn test_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // single line
    cx.set_shared_state(indoc! {"
            The quick brown
            fox ˇjumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v w y").await;
    cx.shared_clipboard().await.assert_eq("jumps o");
    cx.set_shared_state(indoc! {"
            The quick brown
            fox jumps oveˇr
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox jumps overjumps ˇo
            the lazy dog"});

    cx.set_shared_state(indoc! {"
            The quick brown
            fox jumps oveˇr
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox jumps ovejumps ˇor
            the lazy dog"});

    // line mode
    cx.set_shared_state(indoc! {"
            The quick brown
            fox juˇmps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d d").await;
    cx.shared_clipboard().await.assert_eq("fox jumps over\n");
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            the laˇzy dog"});
    cx.simulate_shared_keystrokes("p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            the lazy dog
            ˇfox jumps over"});
    cx.simulate_shared_keystrokes("k shift-p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            ˇfox jumps over
            the lazy dog
            fox jumps over"});

    // multiline, cursor to first character of pasted text.
    cx.set_shared_state(indoc! {"
            The quick brown
            fox jumps ˇover
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v j y").await;
    cx.shared_clipboard().await.assert_eq("over\nthe lazy do");

    cx.simulate_shared_keystrokes("p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox jumps oˇover
            the lazy dover
            the lazy dog"});
    cx.simulate_shared_keystrokes("u shift-p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox jumps ˇover
            the lazy doover
            the lazy dog"});
}

#[gpui::test]
async fn test_yank_system_clipboard_never(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_system_clipboard = Some(UseSystemClipboard::Never)
        });
    });

    cx.set_state(
        indoc! {"
                The quick brown
                fox jˇumps over
                the lazy dog"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i w y");
    cx.assert_state(
        indoc! {"
                The quick brown
                fox ˇjumps over
                the lazy dog"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
                The quick brown
                fox jjumpˇsumps over
                the lazy dog"},
        Mode::Normal,
    );
    assert_eq!(cx.read_from_clipboard(), None);
}

#[gpui::test]
async fn test_yank_system_clipboard_on_yank(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_system_clipboard = Some(UseSystemClipboard::OnYank)
        });
    });

    // copy in visual mode
    cx.set_state(
        indoc! {"
                The quick brown
                fox jˇumps over
                the lazy dog"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i w y");
    cx.assert_state(
        indoc! {"
                The quick brown
                fox ˇjumps over
                the lazy dog"},
        Mode::Normal,
    );
    cx.simulate_keystrokes("p");
    cx.assert_state(
        indoc! {"
                The quick brown
                fox jjumpˇsumps over
                the lazy dog"},
        Mode::Normal,
    );
    assert_eq!(
        cx.read_from_clipboard().map(|item| item.text().unwrap()),
        Some("jumps".into())
    );
    cx.simulate_keystrokes("d d p");
    cx.assert_state(
        indoc! {"
                The quick brown
                the lazy dog
                ˇfox jjumpsumps over"},
        Mode::Normal,
    );
    assert_eq!(
        cx.read_from_clipboard().map(|item| item.text().unwrap()),
        Some("jumps".into())
    );
    cx.write_to_clipboard(ClipboardItem::new_string("test-copy".to_string()));
    cx.simulate_keystrokes("shift-p");
    cx.assert_state(
        indoc! {"
                The quick brown
                the lazy dog
                test-copˇyfox jjumpsumps over"},
        Mode::Normal,
    );
}

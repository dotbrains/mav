use crate::{
    state::{Mode, Register},
    test::{NeovimBackedTestContext, VimTestContext},
};
use gpui::ClipboardItem;
use indoc::indoc;
use language::{LanguageName, language_settings::LanguageSettingsContent};
use settings::{SettingsStore, UseSystemClipboard};
#[gpui::test]
async fn test_paste_count(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            onˇe
            two
            three
        "})
        .await;
    cx.simulate_shared_keystrokes("y y 3 p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            one
            ˇone
            one
            one
            two
            three
        "});

    cx.set_shared_state(indoc! {"
            one
            ˇtwo
            three
        "})
        .await;
    cx.simulate_shared_keystrokes("y $ $ 3 p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            one
            twotwotwotwˇo
            three
        "});
}

#[gpui::test]
async fn test_paste_system_clipboard_never(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_system_clipboard = Some(UseSystemClipboard::Never)
        });
    });

    cx.set_state(
        indoc! {"
                ˇThe quick brown
                fox jumps over
                the lazy dog"},
        Mode::Normal,
    );

    cx.write_to_clipboard(ClipboardItem::new_string("something else".to_string()));

    cx.simulate_keystrokes("d d");
    cx.assert_state(
        indoc! {"
                ˇfox jumps over
                the lazy dog"},
        Mode::Normal,
    );

    cx.simulate_keystrokes("shift-v p");
    cx.assert_state(
        indoc! {"
                ˇThe quick brown
                the lazy dog"},
        Mode::Normal,
    );

    cx.simulate_keystrokes("shift-v");
    cx.dispatch_action(editor::actions::Paste);
    cx.assert_state(
        indoc! {"
                ˇsomething else
                the lazy dog"},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_editor_paste_visual_preserves_system_clipboard(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
                The quick brown
                fox ˇjumps over
                the lazy dog"},
        Mode::Normal,
    );

    // Put known content on the system clipboard
    cx.write_to_clipboard(ClipboardItem::new_string("from clipboard".to_string()));

    // Select "jumps" in visual mode, then editor::Paste (Cmd-V / Ctrl-V)
    cx.simulate_keystrokes("v i w");
    cx.dispatch_action(editor::actions::Paste);

    // The selected text should be replaced with clipboard content
    cx.assert_state(
        indoc! {"
                The quick brown
                fox from clipboarˇd over
                the lazy dog"},
        Mode::Normal,
    );

    // System clipboard must still hold the original value, not "jumps"
    assert_eq!(
        cx.read_from_clipboard().map(|item| item.text().unwrap()),
        Some("from clipboard".into()),
    );
}

#[gpui::test]
async fn test_numbered_registers(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_system_clipboard = Some(UseSystemClipboard::Never)
        });
    });

    cx.set_shared_state(indoc! {"
                The quick brown
                fox jˇumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("y y \" 0 p").await;
    cx.shared_register('0').await.assert_eq("fox jumps over\n");
    cx.shared_register('"').await.assert_eq("fox jumps over\n");

    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                fox jumps over
                ˇfox jumps over
                the lazy dog"});
    cx.simulate_shared_keystrokes("k k d d").await;
    cx.shared_register('0').await.assert_eq("fox jumps over\n");
    cx.shared_register('1').await.assert_eq("The quick brown\n");
    cx.shared_register('"').await.assert_eq("The quick brown\n");

    cx.simulate_shared_keystrokes("d d shift-g d d").await;
    cx.shared_register('0').await.assert_eq("fox jumps over\n");
    cx.shared_register('3').await.assert_eq("The quick brown\n");
    cx.shared_register('2').await.assert_eq("fox jumps over\n");
    cx.shared_register('1').await.assert_eq("the lazy dog\n");

    cx.shared_state().await.assert_eq(indoc! {"
    ˇfox jumps over"});

    cx.simulate_shared_keystrokes("d d \" 3 p p \" 1 p").await;
    cx.set_shared_state(indoc! {"
                The quick brown
                fox jumps over
                ˇthe lazy dog"})
        .await;
}

#[gpui::test]
async fn test_named_registers(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_system_clipboard = Some(UseSystemClipboard::Never)
        });
    });

    cx.set_shared_state(indoc! {"
                The quick brown
                fox jˇumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("\" a d a w").await;
    cx.shared_register('a').await.assert_eq("jumps ");
    cx.simulate_shared_keystrokes("\" shift-a d i w").await;
    cx.shared_register('a').await.assert_eq("jumps over");
    cx.shared_register('"').await.assert_eq("jumps over");
    cx.simulate_shared_keystrokes("\" a p").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                fox jumps oveˇr
                the lazy dog"});
    cx.simulate_shared_keystrokes("\" a d a w").await;
    cx.shared_register('a').await.assert_eq(" over");
}

#[gpui::test]
async fn test_special_registers(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.vim.get_or_insert_default().use_system_clipboard = Some(UseSystemClipboard::Never)
        });
    });

    cx.set_shared_state(indoc! {"
                The quick brown
                fox jˇumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d i w").await;
    cx.shared_register('-').await.assert_eq("jumps");
    cx.simulate_shared_keystrokes("\" _ d d").await;
    cx.shared_register('_').await.assert_eq("");

    cx.simulate_shared_keystrokes("shift-v \" _ y w").await;
    cx.shared_register('"').await.assert_eq("jumps");

    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                the ˇlazy dog"});
    cx.simulate_shared_keystrokes("\" \" d ^").await;
    cx.shared_register('0').await.assert_eq("the ");
    cx.shared_register('"').await.assert_eq("the ");

    cx.simulate_shared_keystrokes("^ \" + d $").await;
    cx.shared_clipboard().await.assert_eq("lazy dog");
    cx.shared_register('"').await.assert_eq("lazy dog");

    cx.simulate_shared_keystrokes("/ d o g enter").await;
    cx.shared_register('/').await.assert_eq("dog");
    cx.simulate_shared_keystrokes("\" / shift-p").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                doˇg"});

    // not testing nvim as it doesn't have a filename
    cx.simulate_keystrokes("\" % p");
    #[cfg(not(target_os = "windows"))]
    cx.assert_state(
        indoc! {"
                    The quick brown
                    dogdir/file.rˇs"},
        Mode::Normal,
    );
    #[cfg(target_os = "windows")]
    cx.assert_state(
        indoc! {"
                    The quick brown
                    dogdir\\file.rˇs"},
        Mode::Normal,
    );
}

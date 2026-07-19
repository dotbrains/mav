use gpui::ClipboardItem;
use indoc::indoc;

use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};

#[gpui::test]
async fn test_enter_and_exit_replace_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.simulate_keystrokes("shift-r");
    assert_eq!(cx.mode(), Mode::Replace);
    cx.simulate_keystrokes("escape");
    assert_eq!(cx.mode(), Mode::Normal);
}

#[gpui::test]
#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
async fn test_replace_mode(cx: &mut gpui::TestAppContext) {
    let mut cx: NeovimBackedTestContext = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        ˇThe quick brown
        fox jumps over
        the lazy dog."})
        .await;
    cx.simulate_shared_keystrokes("shift-r O n e").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Oneˇ quick brown
        fox jumps over
        the lazy dog."});

    cx.set_shared_state(indoc! {"
        The quick browˇn
        fox jumps over
        the lazy dog."})
        .await;
    cx.simulate_shared_keystrokes("shift-r O n e").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick browOneˇ
        fox jumps over
        the lazy dog."});

    cx.set_shared_state(indoc! {"
    The quick brown
    ˇ
    fox jumps over
    the lazy dog."})
        .await;
    cx.simulate_shared_keystrokes("shift-r O n e").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick brown
        Oneˇ
        fox jumps over
        the lazy dog."});

    cx.set_shared_state(indoc! {"
        The quˇick brown
        fox jumps over
        the lazy dog."})
        .await;
    cx.simulate_shared_keystrokes("shift-r enter O n e").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The qu
        Oneˇ brown
        fox jumps over
        the lazy dog."});

    cx.set_state(
        indoc! {"
        ˇThe quick brown
        fox jumps over
        the lazy ˇdog."},
        Mode::Normal,
    );
    cx.simulate_keystrokes("shift-r O n e");
    cx.assert_state(
        indoc! {"
        Oneˇ quick brown
        fox jumps over
        the lazy Oneˇ."},
        Mode::Replace,
    );
    cx.simulate_keystrokes("enter T w o");
    cx.assert_state(
        indoc! {"
        One
        Twoˇck brown
        fox jumps over
        the lazy One
        Twoˇ"},
        Mode::Replace,
    );
}

#[gpui::test]
async fn test_replace_mode_with_counts(cx: &mut gpui::TestAppContext) {
    let mut cx: NeovimBackedTestContext = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello\n").await;
    cx.simulate_shared_keystrokes("3 shift-r - escape").await;
    cx.shared_state().await.assert_eq("--ˇ-lo\n");

    cx.set_shared_state("ˇhello\n").await;
    cx.simulate_shared_keystrokes("3 shift-r a b c escape")
        .await;
    cx.shared_state().await.assert_eq("abcabcabˇc\n");
}

#[gpui::test]
async fn test_replace_mode_repeat(cx: &mut gpui::TestAppContext) {
    let mut cx: NeovimBackedTestContext = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇhello world\n").await;
    cx.simulate_shared_keystrokes("shift-r - - - escape 4 l .")
        .await;
    cx.shared_state().await.assert_eq("---lo --ˇ-ld\n");
}

#[gpui::test]
async fn test_replace_mode_undo(cx: &mut gpui::TestAppContext) {
    let mut cx: NeovimBackedTestContext = NeovimBackedTestContext::new(cx).await;

    const UNDO_REPLACE_EXAMPLES: &[&str] = &[
        "ˇThe quick brown fox jumps over the lazy dog.",
        indoc! {"
            The quick browˇn
            fox jumps over
            the lazy dog."
        },
        indoc! {"
            The quick brown
            ˇ
            fox jumps over
            the lazy dog."
        },
    ];

    for example in UNDO_REPLACE_EXAMPLES {
        cx.simulate("shift-r O n e backspace backspace backspace", example)
            .await
            .assert_matches();
        cx.simulate("shift-r O enter e backspace backspace backspace", example)
            .await
            .assert_matches();
        cx.simulate(
            "shift-r O enter n enter e backspace backspace backspace backspace backspace",
            example,
        )
        .await
        .assert_matches();
    }
}

#[gpui::test]
async fn test_replace_multicursor(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇabcˇabcabc", Mode::Normal);
    cx.simulate_keystrokes("shift-r 1 2 3 4");
    cx.assert_state("1234ˇ234ˇbc", Mode::Replace);
    assert_eq!(cx.mode(), Mode::Replace);
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace");
    cx.assert_state("ˇabˇcabcabc", Mode::Replace);
}

#[gpui::test]
async fn test_replace_undo(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇaaaa", Mode::Normal);
    cx.simulate_keystrokes("0 shift-r b b b escape u");
    cx.assert_state("ˇaaaa", Mode::Normal);
}

#[gpui::test]
async fn test_exchange_separate_range(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇhello world", Mode::Normal);
    cx.simulate_keystrokes("c x i w w c x i w");
    cx.assert_state("world ˇhello", Mode::Normal);
}

#[gpui::test]
async fn test_exchange_complete_overlap(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇhello world", Mode::Normal);
    cx.simulate_keystrokes("c x x w c x i w");
    cx.assert_state("ˇworld", Mode::Normal);

    cx.set_state("ˇhello world", Mode::Normal);
    cx.simulate_keystrokes("c x i w c x x");
    cx.assert_state("ˇhello", Mode::Normal);
}

#[gpui::test]
async fn test_exchange_partial_overlap(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇhello world", Mode::Normal);
    cx.simulate_keystrokes("c x t r w c x i w");
    cx.assert_state("hello ˇworld", Mode::Normal);
}

#[gpui::test]
async fn test_clear_exchange_clears_operator(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇirrelevant", Mode::Normal);
    cx.simulate_keystrokes("c x c");

    assert_eq!(cx.active_operator(), None);
}

#[gpui::test]
async fn test_clear_exchange(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇhello world", Mode::Normal);
    cx.simulate_keystrokes("c x i w c x c");

    cx.update_editor(|editor, window, cx| {
        let highlights = editor.all_text_background_highlights(window, cx);
        assert_eq!(0, highlights.len());
    });
}

#[gpui::test]
async fn test_paste_replace(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(indoc! {"ˇ123"}, Mode::Replace);
    cx.write_to_clipboard(ClipboardItem::new_string("456".to_string()));
    cx.dispatch_action(editor::actions::Paste);
    cx.assert_state(indoc! {"45ˇ6"}, Mode::Replace);

    cx.set_state(indoc! {"ˇ123"}, Mode::Replace);
    cx.write_to_clipboard(ClipboardItem::new_string("4567".to_string()));
    cx.dispatch_action(editor::actions::Paste);
    cx.assert_state(indoc! {"ˇ123"}, Mode::Replace);
}

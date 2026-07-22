use gpui::{KeyBinding, TestAppContext, UpdateGlobal};
use indoc::indoc;
use settings::SettingsStore;

use crate::{
    motion,
    state::Mode::{self},
    test::{NeovimBackedTestContext, VimTestContext},
};
use language;

async fn test_yank_line_with_trailing_newline(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("heˇllo\n").await;
    cx.simulate_shared_keystrokes("y y p").await;
    cx.shared_state().await.assert_eq("hello\nˇhello\n");
}

#[gpui::test]
async fn test_yank_line_without_trailing_newline(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("heˇllo").await;
    cx.simulate_shared_keystrokes("y y p").await;
    cx.shared_state().await.assert_eq("hello\nˇhello");
}

#[gpui::test]
async fn test_yank_multiline_without_trailing_newline(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("heˇllo\nhello").await;
    cx.simulate_shared_keystrokes("2 y y p").await;
    cx.shared_state()
        .await
        .assert_eq("hello\nˇhello\nhello\nhello");
}

#[gpui::test]
async fn test_dd_then_paste_without_trailing_newline(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("heˇllo").await;
    cx.simulate_shared_keystrokes("d d").await;
    cx.shared_state().await.assert_eq("ˇ");
    cx.simulate_shared_keystrokes("p p").await;
    cx.shared_state().await.assert_eq("\nhello\nˇhello");
}

#[gpui::test]
async fn test_visual_mode_insert_before_after(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("heˇllo").await;
    cx.simulate_shared_keystrokes("v i w shift-i").await;
    cx.shared_state().await.assert_eq("ˇhello");

    cx.set_shared_state(indoc! {"
        The quick brown
        fox ˇjumps over
        the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v shift-i").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick brown
        ˇfox jumps over
        the lazy dog"});

    cx.set_shared_state(indoc! {"
        The quick brown
        fox ˇjumps over
        the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v shift-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
        The quick brown
        fox jˇumps over
        the lazy dog"});
}

#[gpui::test]
async fn test_jump_list(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        ˇfn a() { }





        fn b() { }





        fn b() { }"})
        .await;
    cx.simulate_shared_keystrokes("3 }").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-o").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-i").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("1 1 k").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-o").await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_undo_last_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        ˇfn a() { }
        fn a() { }
        fn a() { }
    "})
        .await;
    // do a jump to reset vim's undo grouping
    cx.simulate_shared_keystrokes("shift-g").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("r a").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("g g shift-u").await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_undo_last_line_newline(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        ˇfn a() { }
        fn a() { }
        fn a() { }
    "})
        .await;
    // do a jump to reset vim's undo grouping
    cx.simulate_shared_keystrokes("shift-g k").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("o h e l l o escape").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
}

#[gpui::test]
async fn test_undo_last_line_newline_many_changes(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
        ˇfn a() { }
        fn a() { }
        fn a() { }
    "})
        .await;
    // do a jump to reset vim's undo grouping
    cx.simulate_shared_keystrokes("x shift-g k").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("x f a x f { x").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("shift-u").await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_undo_last_line_multicursor(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
        ˇone two ˇone
        two ˇone two
    "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("3 r a");
    cx.assert_state(
        indoc! {"
        aaˇa two aaˇa
        two aaˇa two
    "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("escape escape");
    cx.simulate_keystrokes("shift-u");
    cx.set_state(
        indoc! {"
        onˇe two onˇe
        two onˇe two
    "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_go_to_tab_with_count(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Open 4 tabs.
    cx.simulate_keystrokes(": tabnew");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes(": tabnew");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes(": tabnew");
    cx.simulate_keystrokes("enter");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.items(cx).count(), 4);
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 3);
    });

    cx.simulate_keystrokes("1 g t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 0);
    });

    cx.simulate_keystrokes("3 g t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 2);
    });

    cx.simulate_keystrokes("4 g t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 3);
    });

    cx.simulate_keystrokes("1 g t");
    cx.simulate_keystrokes("g t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 1);
    });
}

#[gpui::test]
async fn test_go_to_previous_tab_with_count(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Open 4 tabs.
    cx.simulate_keystrokes(": tabnew");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes(": tabnew");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes(": tabnew");
    cx.simulate_keystrokes("enter");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.items(cx).count(), 4);
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 3);
    });

    cx.simulate_keystrokes("2 g shift-t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 1);
    });

    cx.simulate_keystrokes("g shift-t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 0);
    });

    // Wraparound: gT from first tab should go to last.
    cx.simulate_keystrokes("g shift-t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 3);
    });

    cx.simulate_keystrokes("6 g shift-t");
    cx.workspace(|workspace, _, cx| {
        assert_eq!(workspace.active_pane().read(cx).active_item_index(), 1);
    });
}

#[gpui::test]
async fn test_temporary_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // Test jumping to the end of the line ($).
    cx.set_shared_state(indoc! {"lorem ˇipsum"}).await;
    cx.simulate_shared_keystrokes("i").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("ctrl-o $").await;
    cx.shared_state().await.assert_eq(indoc! {"lorem ipsumˇ"});

    // Test jumping to the next word.
    cx.set_shared_state(indoc! {"loremˇ ipsum dolor"}).await;
    cx.simulate_shared_keystrokes("a").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("a n d space ctrl-o w").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {"lorem and ipsum ˇdolor"});

    // Test yanking to end of line ($).
    cx.set_shared_state(indoc! {"lorem ˇipsum dolor"}).await;
    cx.simulate_shared_keystrokes("i").await;
    cx.shared_state().await.assert_matches();
    cx.simulate_shared_keystrokes("a n d space ctrl-o y $")
        .await;
    cx.shared_state()
        .await
        .assert_eq(indoc! {"lorem and ˇipsum dolor"});
}

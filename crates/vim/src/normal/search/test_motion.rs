use std::time::Duration;

use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
use editor::{DisplayPoint, display_map::DisplayRow};

use indoc::indoc;
use search::BufferSearchBar;
use settings::SettingsStore;

#[test]
fn test_replacement_parse_escaped_dollar() {
    let parsed = crate::normal::search::Replacement::parse(r"/\$test/\$rest/g".chars().peekable())
        .expect("parse should succeed");

    assert_eq!(parsed.search, r"\$test");
    assert_eq!(parsed.replacement, "$$rest");
    assert!(parsed.flag_g);
}

#[gpui::test]
async fn test_move_to_next(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇhi\nhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("*");
    cx.run_until_parked();
    cx.assert_state("hi\nhigh\nˇhi\n", Mode::Normal);

    cx.simulate_keystrokes("*");
    cx.run_until_parked();
    cx.assert_state("ˇhi\nhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("#");
    cx.run_until_parked();
    cx.assert_state("hi\nhigh\nˇhi\n", Mode::Normal);

    cx.simulate_keystrokes("#");
    cx.run_until_parked();
    cx.assert_state("ˇhi\nhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("2 *");
    cx.run_until_parked();
    cx.assert_state("ˇhi\nhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("g *");
    cx.run_until_parked();
    cx.assert_state("hi\nˇhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("n");
    cx.assert_state("hi\nhigh\nˇhi\n", Mode::Normal);

    cx.simulate_keystrokes("g #");
    cx.run_until_parked();
    cx.assert_state("hi\nˇhigh\nhi\n", Mode::Normal);
}

#[gpui::test]
async fn test_move_to_next_with_no_search_wrap(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| s.editor.search_wrap = Some(false));
    });

    cx.set_state("ˇhi\nhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("*");
    cx.run_until_parked();
    cx.assert_state("hi\nhigh\nˇhi\n", Mode::Normal);

    cx.simulate_keystrokes("*");
    cx.run_until_parked();
    cx.assert_state("hi\nhigh\nˇhi\n", Mode::Normal);

    cx.simulate_keystrokes("#");
    cx.run_until_parked();
    cx.assert_state("ˇhi\nhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("3 *");
    cx.run_until_parked();
    cx.assert_state("ˇhi\nhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("g *");
    cx.run_until_parked();
    cx.assert_state("hi\nˇhigh\nhi\n", Mode::Normal);

    cx.simulate_keystrokes("n");
    cx.assert_state("hi\nhigh\nˇhi\n", Mode::Normal);

    cx.simulate_keystrokes("g #");
    cx.run_until_parked();
    cx.assert_state("hi\nˇhigh\nhi\n", Mode::Normal);
}

#[gpui::test]
async fn test_search(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("aa\nbˇb\ncc\ncc\ncc\n", Mode::Normal);
    cx.simulate_keystrokes("/ c c");

    let search_bar = cx.workspace(|workspace, _, cx| {
        workspace
            .active_pane()
            .read(cx)
            .toolbar()
            .read(cx)
            .item_of_type::<BufferSearchBar>()
            .expect("Buffer search bar should be deployed")
    });

    cx.update_entity(search_bar, |bar, _window, cx| {
        assert_eq!(bar.query(cx), "cc");
    });

    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        let highlights = editor.all_text_background_highlights(window, cx);
        assert_eq!(3, highlights.len());
        assert_eq!(
            DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(2), 2),
            highlights[0].0
        )
    });

    cx.simulate_keystrokes("enter");
    cx.assert_state("aa\nbb\nˇcc\ncc\ncc\n", Mode::Normal);

    // n to go to next/N to go to previous
    cx.simulate_keystrokes("n");
    cx.assert_state("aa\nbb\ncc\nˇcc\ncc\n", Mode::Normal);
    cx.simulate_keystrokes("shift-n");
    cx.assert_state("aa\nbb\nˇcc\ncc\ncc\n", Mode::Normal);

    // ?<enter> to go to previous
    cx.simulate_keystrokes("? enter");
    cx.assert_state("aa\nbb\ncc\ncc\nˇcc\n", Mode::Normal);
    cx.simulate_keystrokes("? enter");
    cx.assert_state("aa\nbb\ncc\nˇcc\ncc\n", Mode::Normal);

    // /<enter> to go to next
    cx.simulate_keystrokes("/ enter");
    cx.assert_state("aa\nbb\ncc\ncc\nˇcc\n", Mode::Normal);

    // ?{search}<enter> to search backwards
    cx.simulate_keystrokes("? b enter");
    cx.assert_state("aa\nbˇb\ncc\ncc\ncc\n", Mode::Normal);

    // works with counts
    cx.simulate_keystrokes("4 / c");
    cx.simulate_keystrokes("enter");
    cx.assert_state("aa\nbb\ncc\ncˇc\ncc\n", Mode::Normal);

    // check that searching resumes from cursor, not previous match
    cx.set_state("ˇaa\nbb\ndd\ncc\nbb\n", Mode::Normal);
    cx.simulate_keystrokes("/ d");
    cx.simulate_keystrokes("enter");
    cx.assert_state("aa\nbb\nˇdd\ncc\nbb\n", Mode::Normal);
    cx.update_editor(|editor, window, cx| {
        editor.move_to_beginning(&Default::default(), window, cx)
    });
    cx.assert_state("ˇaa\nbb\ndd\ncc\nbb\n", Mode::Normal);
    cx.simulate_keystrokes("/ b");
    cx.simulate_keystrokes("enter");
    cx.assert_state("aa\nˇbb\ndd\ncc\nbb\n", Mode::Normal);

    // check that searching switches to normal mode if in visual mode
    cx.set_state("ˇone two one", Mode::Normal);
    cx.simulate_keystrokes("v l l");
    cx.assert_editor_state("«oneˇ» two one");
    cx.simulate_keystrokes("*");
    cx.assert_state("one two ˇone", Mode::Normal);

    // check that a backward search after last match works correctly
    cx.set_state("aa\naa\nbbˇ", Mode::Normal);
    cx.simulate_keystrokes("? a a");
    cx.simulate_keystrokes("enter");
    cx.assert_state("aa\nˇaa\nbb", Mode::Normal);

    // check that searching with unable search wrap
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| s.editor.search_wrap = Some(false));
    });
    cx.set_state("aa\nbˇb\ncc\ncc\ncc\n", Mode::Normal);
    cx.simulate_keystrokes("/ c c enter");

    cx.assert_state("aa\nbb\nˇcc\ncc\ncc\n", Mode::Normal);

    // n to go to next/N to go to previous
    cx.simulate_keystrokes("n");
    cx.assert_state("aa\nbb\ncc\nˇcc\ncc\n", Mode::Normal);
    cx.simulate_keystrokes("shift-n");
    cx.assert_state("aa\nbb\nˇcc\ncc\ncc\n", Mode::Normal);

    // ?<enter> to go to previous
    cx.simulate_keystrokes("? enter");
    cx.assert_state("aa\nbb\nˇcc\ncc\ncc\n", Mode::Normal);
    cx.simulate_keystrokes("? enter");
    cx.assert_state("aa\nbb\nˇcc\ncc\ncc\n", Mode::Normal);
}

#[gpui::test]
async fn test_non_vim_search(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, false).await;
    cx.cx.set_state("ˇone one one one");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();

    cx.assert_editor_state("«oneˇ» one one one");
    cx.simulate_keystrokes("enter");
    cx.assert_editor_state("one «oneˇ» one one");
    cx.simulate_keystrokes("shift-enter");
    cx.assert_editor_state("«oneˇ» one one one");
}

#[gpui::test]
async fn test_non_vim_search_in_vim_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.cx.set_state("ˇone one one one");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();

    cx.assert_state("«oneˇ» one one one", Mode::Visual);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.assert_state("one «oneˇ» one one", Mode::Visual);
    cx.simulate_keystrokes("shift-enter");
    cx.run_until_parked();
    cx.assert_state("«oneˇ» one one one", Mode::Visual);

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.assert_state("«oneˇ» one one one", Mode::Visual);
}

#[gpui::test]
async fn test_non_vim_search_in_vim_insert_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇone one one one", Mode::Insert);
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();

    cx.assert_state("«oneˇ» one one one", Mode::Insert);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.assert_state("one «oneˇ» one one", Mode::Insert);

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.assert_state("one «oneˇ» one one", Mode::Insert);
}

#[gpui::test]
async fn test_n_after_cmd_f_search(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇone two one two one", Mode::Normal);
    cx.run_until_parked();

    // Use cmd+f to search (non-vim style)
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // Now use n to go to next match — should move cursor, not create selection
    cx.simulate_keystrokes("n");
    cx.run_until_parked();
    cx.assert_state("one two ˇone two one", Mode::Normal);

    cx.simulate_keystrokes("n");
    cx.run_until_parked();
    cx.assert_state("one two one two ˇone", Mode::Normal);
}

#[gpui::test]
async fn test_star_after_cmd_f_search(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state("ˇone two one two one", Mode::Normal);
    cx.run_until_parked();

    // Use cmd+f to search (non-vim style)
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // Now use * to search under cursor — should move cursor, not create selection
    cx.simulate_keystrokes("*");
    cx.run_until_parked();
    cx.assert_state("one two ˇone two one", Mode::Normal);
}

#[gpui::test]
async fn test_visual_star_hash(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇa.c. abcd a.c. abcd").await;
    cx.simulate_shared_keystrokes("v 3 l *").await;
    cx.shared_state().await.assert_eq("a.c. abcd ˇa.c. abcd");
}

#[gpui::test]
async fn test_d_search(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇa.c. abcd a.c. abcd").await;
    cx.simulate_shared_keystrokes("d / c d").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq("ˇcd a.c. abcd");
}

#[gpui::test]
async fn test_backwards_n(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇa b a b a b a").await;
    cx.simulate_shared_keystrokes("*").await;
    cx.simulate_shared_keystrokes("n").await;
    cx.shared_state().await.assert_eq("a b a b ˇa b a");
    cx.simulate_shared_keystrokes("#").await;
    cx.shared_state().await.assert_eq("a b ˇa b a b a");
    cx.simulate_shared_keystrokes("n").await;
    cx.shared_state().await.assert_eq("ˇa b a b a b a");
}

#[gpui::test]
async fn test_v_search(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇa.c. abcd a.c. abcd").await;
    cx.simulate_shared_keystrokes("v / c d").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq("«a.c. abcˇ»d a.c. abcd");

    cx.set_shared_state("a a aˇ a a a").await;
    cx.simulate_shared_keystrokes("v / a").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq("a a a« aˇ» a a");
    cx.simulate_shared_keystrokes("/ enter").await;
    cx.shared_state().await.assert_eq("a a a« a aˇ» a");
    cx.simulate_shared_keystrokes("? enter").await;
    cx.shared_state().await.assert_eq("a a a« aˇ» a a");
    cx.simulate_shared_keystrokes("? enter").await;
    cx.shared_state().await.assert_eq("a a «ˇa »a a a");
    cx.simulate_shared_keystrokes("/ enter").await;
    cx.shared_state().await.assert_eq("a a a« aˇ» a a");
    cx.simulate_shared_keystrokes("/ enter").await;
    cx.shared_state().await.assert_eq("a a a« a aˇ» a");
}

#[gpui::test]
async fn test_v_search_aa(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇaa aa").await;
    cx.simulate_shared_keystrokes("v / a a").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq("«aa aˇ»a");
}

#[gpui::test]
async fn test_visual_block_search(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "ˇone two
         three four
         five six
         "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v j / f").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«one twoˇ»
         «three fˇ»our
         five six
         "
    });
}

use super::*;

#[gpui::test]
async fn test_helix_select_next_match_wrapping(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Three occurrences of "one". After selecting all three with `n n`,
    // pressing `n` again wraps the search to the first occurrence.
    // The prior selections (at higher offsets) are chained before the
    // wrapped selection (at a lower offset), producing unsorted anchors
    // that cause `rope::Cursor::summary` to panic with
    // "cannot summarize backward".
    cx.set_state("ˇhello two one two one two one", Mode::HelixSelect);
    cx.simulate_keystrokes("/ o n e");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("n n n");
    // Should not panic; all three occurrences should remain selected.
    cx.assert_state("hello two «oneˇ» two «oneˇ» two «oneˇ»", Mode::HelixSelect);
}

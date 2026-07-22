use super::*;

#[gpui::test]
async fn test_helix_select_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // v g l d should delete to end of line without consuming the newline
    cx.set_state("ˇThe quick brown\nfox jumps over", Mode::HelixNormal);
    cx.simulate_keystrokes("v g l d");
    cx.assert_state("ˇ\nfox jumps over", Mode::HelixNormal);

    // same from the middle of a line — cursor lands on the last
    // remaining character (the space) after delete
    cx.set_state("The ˇquick brown\nfox jumps over", Mode::HelixNormal);
    cx.simulate_keystrokes("v g l d");
    cx.assert_state("Theˇ \nfox jumps over", Mode::HelixNormal);
}

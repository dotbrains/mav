use super::*;

#[gpui::test]
async fn test_helix_select_mode_motion_multiple_cursors(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    assert_eq!(cx.mode(), Mode::Normal);
    cx.enable_helix();

    // Start with multiple cursors (no selections)
    cx.set_state("ˇhello\nˇworld", Mode::HelixNormal);

    // Enter select mode and move right twice
    cx.simulate_keystrokes("v l l");

    // Each cursor should independently create and extend its own selection
    cx.assert_state("«helˇ»lo\n«worˇ»ld", Mode::HelixSelect);
}

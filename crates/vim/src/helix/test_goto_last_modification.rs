use super::*;

#[gpui::test]
async fn test_goto_last_modification(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Make a modification at a specific location
    cx.set_state("line one\nline ˇtwo\nline three", Mode::HelixNormal);
    cx.assert_state("line one\nline ˇtwo\nline three", Mode::HelixNormal);
    cx.simulate_keystrokes("i");
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("i");
    cx.simulate_keystrokes("m o d i f i e d space");
    cx.simulate_keystrokes("escape");

    // TODO: this fails, because state is no longer helix
    cx.assert_state(
        "line one\nline modified ˇtwo\nline three",
        Mode::HelixNormal,
    );

    // Move cursor away from the modification
    cx.simulate_keystrokes("up");

    // Use "g ." to go back to last modification
    cx.simulate_keystrokes("g .");

    // Verify we're back at the modification location and still in HelixNormal mode
    cx.assert_state(
        "line one\nline modifiedˇ two\nline three",
        Mode::HelixNormal,
    );
}

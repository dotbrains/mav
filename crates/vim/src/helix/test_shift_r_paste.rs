use super::*;

#[gpui::test]
async fn test_shift_r_paste(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // First copy some text to clipboard
    cx.set_state("«hello worldˇ»", Mode::HelixNormal);
    cx.simulate_keystrokes("y");

    // Test paste with shift-r on single cursor
    cx.set_state("foo ˇbar", Mode::HelixNormal);
    cx.simulate_keystrokes("shift-r");

    cx.assert_state("foo hello worldˇbar", Mode::HelixNormal);

    // Test paste with shift-r on selection
    cx.set_state("foo «barˇ» baz", Mode::HelixNormal);
    cx.simulate_keystrokes("shift-r");

    cx.assert_state("foo hello worldˇ baz", Mode::HelixNormal);
}

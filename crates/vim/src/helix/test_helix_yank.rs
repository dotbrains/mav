use super::*;

#[gpui::test]
async fn test_helix_yank(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Test yanking current character with no selection
    cx.set_state("hello ˇworld", Mode::HelixNormal);
    cx.simulate_keystrokes("y");

    // Test cursor remains at the same position after yanking single character
    cx.assert_state("hello ˇworld", Mode::HelixNormal);
    cx.shared_clipboard().assert_eq("w");

    // Move cursor and yank another character
    cx.simulate_keystrokes("l");
    cx.simulate_keystrokes("y");
    cx.shared_clipboard().assert_eq("o");

    // Test yanking with existing selection
    cx.set_state("hello «worlˇ»d", Mode::HelixNormal);
    cx.simulate_keystrokes("y");
    cx.shared_clipboard().assert_eq("worl");
    cx.assert_state("hello «worlˇ»d", Mode::HelixNormal);

    // Test yanking in select mode character by character
    cx.set_state("hello ˇworld", Mode::HelixNormal);
    cx.simulate_keystroke("v");
    cx.assert_state("hello «wˇ»orld", Mode::HelixSelect);
    cx.simulate_keystroke("y");
    cx.assert_state("hello «wˇ»orld", Mode::HelixNormal);
    cx.shared_clipboard().assert_eq("w");
}

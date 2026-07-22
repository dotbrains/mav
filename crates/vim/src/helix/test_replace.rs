use super::*;

#[gpui::test]
async fn test_replace(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // No selection (single character)
    cx.set_state("ˇaa", Mode::HelixNormal);

    cx.simulate_keystrokes("r x");

    cx.assert_state("ˇxa", Mode::HelixNormal);

    // Cursor at the beginning
    cx.set_state("«ˇaa»", Mode::HelixNormal);

    cx.simulate_keystrokes("r x");

    cx.assert_state("«ˇxx»", Mode::HelixNormal);

    // Cursor at the end
    cx.set_state("«aaˇ»", Mode::HelixNormal);

    cx.simulate_keystrokes("r x");

    cx.assert_state("«xxˇ»", Mode::HelixNormal);

    cx.set_state("«aaˇ»", Mode::HelixSelect);

    cx.simulate_keystrokes("r x");

    cx.assert_state("«xxˇ»", Mode::HelixNormal);
}

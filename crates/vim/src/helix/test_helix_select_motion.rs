use super::*;

#[gpui::test]
async fn test_helix_select_motion(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state("«ˇ»one two three", Mode::HelixSelect);
    cx.simulate_keystrokes("w");
    cx.assert_state("«one ˇ»two three", Mode::HelixSelect);

    cx.set_state("«ˇ»one two three", Mode::HelixSelect);
    cx.simulate_keystrokes("e");
    cx.assert_state("«oneˇ» two three", Mode::HelixSelect);
}

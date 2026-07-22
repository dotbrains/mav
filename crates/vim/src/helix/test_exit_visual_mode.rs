use super::*;

#[gpui::test]
async fn test_exit_visual_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇone two", Mode::Normal);
    cx.simulate_keystrokes("v w");
    cx.assert_state("«one tˇ»wo", Mode::Visual);
    cx.simulate_keystrokes("escape");
    cx.assert_state("one ˇtwo", Mode::Normal);

    cx.enable_helix();
    cx.set_state("ˇone two", Mode::HelixNormal);
    cx.simulate_keystrokes("v w");
    cx.assert_state("«one ˇ»two", Mode::HelixSelect);
    cx.simulate_keystrokes("escape");
    cx.assert_state("«one ˇ»two", Mode::HelixNormal);
}

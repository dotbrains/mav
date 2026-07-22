use super::*;

#[gpui::test]
async fn test_newline_char(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state("aa«\nˇ»bb cc", Mode::HelixNormal);

    cx.simulate_keystroke("w");

    cx.assert_state("aa\n«bb ˇ»cc", Mode::HelixNormal);

    cx.set_state("aa«\nˇ»", Mode::HelixNormal);

    cx.simulate_keystroke("b");

    cx.assert_state("«ˇaa»\n", Mode::HelixNormal);
}

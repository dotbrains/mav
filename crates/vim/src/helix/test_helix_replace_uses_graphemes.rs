use super::*;

#[gpui::test]
async fn test_helix_replace_uses_graphemes(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state("«Hällöˇ» Wörld", Mode::HelixNormal);
    cx.simulate_keystrokes("r 1");
    cx.assert_state("«11111ˇ» Wörld", Mode::HelixNormal);

    cx.set_state("«e\u{301}ˇ»", Mode::HelixNormal);
    cx.simulate_keystrokes("r 1");
    cx.assert_state("«1ˇ»", Mode::HelixNormal);

    cx.set_state("«🙂ˇ»", Mode::HelixNormal);
    cx.simulate_keystrokes("r 1");
    cx.assert_state("«1ˇ»", Mode::HelixNormal);
}

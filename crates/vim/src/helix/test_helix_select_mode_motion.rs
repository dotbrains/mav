use super::*;

#[gpui::test]
async fn test_helix_select_mode_motion(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    assert_eq!(cx.mode(), Mode::Normal);
    cx.enable_helix();

    cx.set_state("ˇhello", Mode::HelixNormal);
    cx.simulate_keystrokes("l v l l");
    cx.assert_state("h«ellˇ»o", Mode::HelixSelect);
}

use super::*;

#[gpui::test]
async fn test_helix_select_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    assert_eq!(cx.mode(), Mode::Normal);
    cx.enable_helix();

    cx.simulate_keystrokes("v");
    assert_eq!(cx.mode(), Mode::HelixSelect);
    cx.simulate_keystrokes("escape");
    assert_eq!(cx.mode(), Mode::HelixNormal);
}

use super::*;

#[gpui::test]
async fn test_insert_mode_stickiness(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    // Make a modification at a specific location
    cx.set_state("ˇhello", Mode::HelixNormal);
    assert_eq!(cx.mode(), Mode::HelixNormal);
    cx.simulate_keystrokes("i");
    assert_eq!(cx.mode(), Mode::Insert);
    cx.simulate_keystrokes("escape");
    assert_eq!(cx.mode(), Mode::HelixNormal);
}

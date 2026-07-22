use super::*;

#[gpui::test]
async fn test_helix_select_append(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state("aˇbcd", Mode::HelixNormal);
    cx.simulate_keystrokes("v a");
    cx.assert_state("abˇcd", Mode::Insert);
}

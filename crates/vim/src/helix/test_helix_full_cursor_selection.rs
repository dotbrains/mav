use super::*;

#[gpui::test]
async fn test_helix_full_cursor_selection(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state("ˇone two three", Mode::HelixNormal);
    cx.simulate_keystrokes("l l v h h h");
    cx.assert_state("«ˇone» two three", Mode::HelixSelect);
}

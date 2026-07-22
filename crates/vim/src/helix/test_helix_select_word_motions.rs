use super::*;

#[gpui::test]
async fn test_helix_select_word_motions(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇone two", Mode::Normal);
    cx.simulate_keystrokes("v w");
    cx.assert_state("«one tˇ»wo", Mode::Visual);

    // In Vim, this selects "t". In helix selections stops just before "t"

    cx.enable_helix();
    cx.set_state("ˇone two", Mode::HelixNormal);
    cx.simulate_keystrokes("v w");
    cx.assert_state("«one ˇ»two", Mode::HelixSelect);
}

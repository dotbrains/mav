use super::*;

#[gpui::test]
async fn test_helix_select_regex(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.enable_helix();

    cx.set_state("ˇone two one", Mode::HelixNormal);
    cx.simulate_keystrokes("x");
    cx.assert_state("«one two oneˇ»", Mode::HelixNormal);
    cx.simulate_keystrokes("s o n e");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.assert_state("«oneˇ» two «oneˇ»", Mode::HelixNormal);

    cx.simulate_keystrokes("x");
    cx.simulate_keystrokes("s");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.assert_state("«oneˇ» two «oneˇ»", Mode::HelixNormal);

    // TODO: change "search_in_selection" to not perform any search when in helix select mode with no selection
    // cx.set_state("ˇstuff one two one", Mode::HelixNormal);
    // cx.simulate_keystrokes("s o n e enter");
    // cx.assert_state("ˇstuff one two one", Mode::HelixNormal);
}

use super::*;

#[gpui::test]
async fn test_helix_select_next_match(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇhello two one two one two one", Mode::Visual);
    cx.simulate_keystrokes("/ o n e");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("n n");
    cx.assert_state("«hello two one two one two oˇ»ne", Mode::Visual);

    cx.set_state("ˇhello two one two one two one", Mode::Normal);
    cx.simulate_keystrokes("/ o n e");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("n n");
    cx.assert_state("hello two one two one two ˇone", Mode::Normal);

    cx.set_state("ˇhello two one two one two one", Mode::Normal);
    cx.simulate_keystrokes("/ o n e");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("n g n g n");
    cx.assert_state("hello two one two «one two oneˇ»", Mode::Visual);

    cx.enable_helix();

    cx.set_state("ˇhello two one two one two one", Mode::HelixNormal);
    cx.simulate_keystrokes("/ o n e");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("n n");
    cx.assert_state("hello two one two one two «oneˇ»", Mode::HelixNormal);

    cx.set_state("ˇhello two one two one two one", Mode::HelixSelect);
    cx.simulate_keystrokes("/ o n e");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("n n");
    cx.assert_state("hello two «oneˇ» two «oneˇ» two «oneˇ»", Mode::HelixSelect);
}

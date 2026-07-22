use super::*;

#[gpui::test]
async fn test_mode_across_command(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("aˇbc", Mode::Normal);
    cx.simulate_keystrokes("ctrl-v");
    assert_eq!(cx.mode(), Mode::VisualBlock);
    cx.simulate_keystrokes("cmd-shift-p escape");
    assert_eq!(cx.mode(), Mode::VisualBlock);
}

#[gpui::test]
async fn test_gn(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("aaˇ aa aa aa aa").await;
    cx.simulate_shared_keystrokes("/ a a enter").await;
    cx.shared_state().await.assert_eq("aa ˇaa aa aa aa");
    cx.simulate_shared_keystrokes("g n").await;
    cx.shared_state().await.assert_eq("aa «aaˇ» aa aa aa");
    cx.simulate_shared_keystrokes("g n").await;
    cx.shared_state().await.assert_eq("aa «aa aaˇ» aa aa");
    cx.simulate_shared_keystrokes("escape d g n").await;
    cx.shared_state().await.assert_eq("aa aa ˇ aa aa");

    cx.set_shared_state("aaˇ aa aa aa aa").await;
    cx.simulate_shared_keystrokes("/ a a enter").await;
    cx.shared_state().await.assert_eq("aa ˇaa aa aa aa");
    cx.simulate_shared_keystrokes("3 g n").await;
    cx.shared_state().await.assert_eq("aa aa aa «aaˇ» aa");

    cx.set_shared_state("aaˇ aa aa aa aa").await;
    cx.simulate_shared_keystrokes("/ a a enter").await;
    cx.shared_state().await.assert_eq("aa ˇaa aa aa aa");
    cx.simulate_shared_keystrokes("g shift-n").await;
    cx.shared_state().await.assert_eq("aa «ˇaa» aa aa aa");
    cx.simulate_shared_keystrokes("g shift-n").await;
    cx.shared_state().await.assert_eq("«ˇaa aa» aa aa aa");
}

#[gpui::test]
async fn test_gl(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("aaˇ aa\naa", Mode::Normal);
    cx.simulate_keystrokes("g l");
    cx.assert_state("«aaˇ» «aaˇ»\naa", Mode::Visual);
    cx.simulate_keystrokes("g >");
    cx.assert_state("«aaˇ» aa\n«aaˇ»", Mode::Visual);
}

#[gpui::test]
async fn test_dgn_repeat(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("aaˇ aa aa aa aa").await;
    cx.simulate_shared_keystrokes("/ a a enter").await;
    cx.shared_state().await.assert_eq("aa ˇaa aa aa aa");
    cx.simulate_shared_keystrokes("d g n").await;

    cx.shared_state().await.assert_eq("aa ˇ aa aa aa");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("aa  ˇ aa aa");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("aa   ˇ aa");
}

#[gpui::test]
async fn test_cgn_repeat(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("aaˇ aa aa aa aa").await;
    cx.simulate_shared_keystrokes("/ a a enter").await;
    cx.shared_state().await.assert_eq("aa ˇaa aa aa aa");
    cx.simulate_shared_keystrokes("c g n x escape").await;
    cx.shared_state().await.assert_eq("aa ˇx aa aa aa");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("aa x ˇx aa aa");
}

#[gpui::test]
async fn test_cgn_nomatch(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("aaˇ aa aa aa aa").await;
    cx.simulate_shared_keystrokes("/ b b enter").await;
    cx.shared_state().await.assert_eq("aaˇ aa aa aa aa");
    cx.simulate_shared_keystrokes("c g n x escape").await;
    cx.shared_state().await.assert_eq("aaˇaa aa aa aa");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("aaˇa aa aa aa");

    cx.set_shared_state("aaˇ bb aa aa aa").await;
    cx.simulate_shared_keystrokes("/ b b enter").await;
    cx.shared_state().await.assert_eq("aa ˇbb aa aa aa");
    cx.simulate_shared_keystrokes("c g n x escape").await;
    cx.shared_state().await.assert_eq("aa ˇx aa aa aa");
    cx.simulate_shared_keystrokes(".").await;
    cx.shared_state().await.assert_eq("aa ˇx aa aa aa");
}

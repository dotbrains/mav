use super::*;

const WORD_LOCATIONS: &str = indoc! {"
        The quick ˇbrowˇnˇ•••
        fox ˇjuˇmpsˇ over
        the lazy dogˇ••
        ˇ
        ˇ
        ˇ
        Thˇeˇ-ˇquˇickˇ ˇbrownˇ•
        ˇ••
        ˇ••
        ˇ  fox-jumpˇs over
        the lazy dogˇ•
        ˇ
        "
};

#[gpui::test]
async fn test_change_word_object(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate_at_each_offset("c i w", WORD_LOCATIONS)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("c i shift-w", WORD_LOCATIONS)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("c a w", WORD_LOCATIONS)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("c a shift-w", WORD_LOCATIONS)
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_delete_word_object(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate_at_each_offset("d i w", WORD_LOCATIONS)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("d i shift-w", WORD_LOCATIONS)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("d a w", WORD_LOCATIONS)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("d a shift-w", WORD_LOCATIONS)
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_visual_word_object(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    /*
            cx.set_shared_state("The quick ˇbrown\nfox").await;
            cx.simulate_shared_keystrokes(["v"]).await;
            cx.assert_shared_state("The quick «bˇ»rown\nfox").await;
            cx.simulate_shared_keystrokes(["i", "w"]).await;
            cx.assert_shared_state("The quick «brownˇ»\nfox").await;
    */
    cx.set_shared_state("The quick brown\nˇ\nfox").await;
    cx.simulate_shared_keystrokes("v").await;
    cx.shared_state()
        .await
        .assert_eq("The quick brown\n«\nˇ»fox");
    cx.simulate_shared_keystrokes("i w").await;
    cx.shared_state()
        .await
        .assert_eq("The quick brown\n«\nˇ»fox");

    cx.simulate_at_each_offset("v i w", WORD_LOCATIONS)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("v i shift-w", WORD_LOCATIONS)
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_word_object_with_count(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇone two three four").await;
    cx.simulate_shared_keystrokes("2 d a w").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state("ˇone two three four").await;
    cx.simulate_shared_keystrokes("d 2 a w").await;
    cx.shared_state().await.assert_matches();

    // WORD (shift-w) ignores punctuation
    cx.set_shared_state("ˇone-two three-four five").await;
    cx.simulate_shared_keystrokes("2 d a shift-w").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state("ˇone two three four five").await;
    cx.simulate_shared_keystrokes("3 d a w").await;
    cx.shared_state().await.assert_matches();

    // Multiplied counts: 2d2aw deletes 4 words (2*2)
    cx.set_shared_state("ˇone two three four five six").await;
    cx.simulate_shared_keystrokes("2 d 2 a w").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state("ˇone two three four").await;
    cx.simulate_shared_keystrokes("2 c a w").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state("ˇone two three four").await;
    cx.simulate_shared_keystrokes("2 y a w p").await;
    cx.shared_state().await.assert_matches();

    // Punctuation: foo-bar is 3 word units (foo, -, bar), so 2aw selects "foo-"
    cx.set_shared_state("  ˇfoo-bar baz").await;
    cx.simulate_shared_keystrokes("2 d a w").await;
    cx.shared_state().await.assert_matches();

    // Trailing whitespace counts as a word unit for iw
    cx.set_shared_state("ˇfoo   ").await;
    cx.simulate_shared_keystrokes("2 d i w").await;
    cx.shared_state().await.assert_matches();

    // Multi-line: count > 1 crosses line boundaries
    cx.set_shared_state("ˇone\ntwo\nthree").await;
    cx.simulate_shared_keystrokes("2 d a w").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state("ˇone\ntwo\nthree\nfour").await;
    cx.simulate_shared_keystrokes("3 d a w").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state("ˇone\ntwo\nthree").await;
    cx.simulate_shared_keystrokes("2 d i w").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state("one ˇtwo\nthree four").await;
    cx.simulate_shared_keystrokes("2 d a w").await;
    cx.shared_state().await.assert_matches();
}

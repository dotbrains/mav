use super::*;

#[gpui::test]
async fn test_visual_delete(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.simulate("v w", "The quick ˇbrown")
        .await
        .assert_matches();

    cx.simulate("v w x", "The quick ˇbrown")
        .await
        .assert_matches();
    cx.simulate(
        "v w j x",
        indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog"},
    )
    .await
    .assert_matches();
    // Test pasting code copied on delete
    cx.simulate_shared_keystrokes("j p").await;
    cx.shared_state().await.assert_matches();

    cx.simulate_at_each_offset(
        "v w j x",
        indoc! {"
            The ˇquick brown
            fox jumps over
            the ˇlazy dog"},
    )
    .await
    .assert_matches();
    cx.simulate_at_each_offset(
        "v b k x",
        indoc! {"
            The ˇquick brown
            fox jumps ˇover
            the ˇlazy dog"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_visual_line_delete(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v x").await;
    cx.shared_state().await.assert_matches();

    // Test pasting code copied on delete
    cx.simulate_shared_keystrokes("p").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state(indoc! {"
            The quick brown
            fox jumps over
            the laˇzy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v x").await;
    cx.shared_state().await.assert_matches();
    cx.shared_clipboard().await.assert_eq("the lazy dog\n");

    cx.set_shared_state(indoc! {"
                            The quˇick brown
                            fox jumps over
                            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v j x").await;
    cx.shared_state().await.assert_matches();
    // Test pasting code copied on delete
    cx.simulate_shared_keystrokes("p").await;
    cx.shared_state().await.assert_matches();

    cx.set_shared_state(indoc! {"
        The ˇlong line
        should not
        crash
        "})
        .await;
    cx.simulate_shared_keystrokes("shift-v $ x").await;
    cx.shared_state().await.assert_matches();
}

#[gpui::test]
async fn test_visual_yank(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("The quick ˇbrown").await;
    cx.simulate_shared_keystrokes("v w y").await;
    cx.shared_state().await.assert_eq("The quick ˇbrown");
    cx.shared_clipboard().await.assert_eq("brown");

    cx.set_shared_state(indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v w j y").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The ˇquick brown
                fox jumps over
                the lazy dog"});
    cx.shared_clipboard().await.assert_eq(indoc! {"
            quick brown
            fox jumps o"});

    cx.set_shared_state(indoc! {"
                The quick brown
                fox jumps over
                the ˇlazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v w j y").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                fox jumps over
                the ˇlazy dog"});
    cx.shared_clipboard().await.assert_eq("lazy d");
    cx.simulate_shared_keystrokes("shift-v y").await;
    cx.shared_clipboard().await.assert_eq("the lazy dog\n");

    cx.set_shared_state(indoc! {"
                The ˇquick brown
                fox jumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v b k y").await;
    cx.shared_state().await.assert_eq(indoc! {"
                ˇThe quick brown
                fox jumps over
                the lazy dog"});
    assert_eq!(
        cx.read_from_clipboard()
            .map(|item| item.text().unwrap())
            .unwrap(),
        "The q"
    );

    cx.set_shared_state(indoc! {"
                The quick brown
                fox ˇjumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v shift-g shift-y")
        .await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                ˇfox jumps over
                the lazy dog"});
    cx.shared_clipboard()
        .await
        .assert_eq("fox jumps over\nthe lazy dog\n");

    cx.set_shared_state(indoc! {"
                The quick brown
                fox ˇjumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("shift-v $ shift-y").await;
    cx.shared_state().await.assert_eq(indoc! {"
                The quick brown
                ˇfox jumps over
                the lazy dog"});
    cx.shared_clipboard().await.assert_eq("fox jumps over\n");
}

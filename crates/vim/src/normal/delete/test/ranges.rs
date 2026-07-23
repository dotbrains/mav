use indoc::indoc;

use crate::{
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
#[gpui::test]
async fn test_delete_to_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d 3 shift-g",
        indoc! {"
            The quick
            brownˇ fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d 3 shift-g",
        indoc! {"
            The quick
            brown fox
            jumps over
            the lˇazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d 2 shift-g",
        indoc! {"
            The quick
            brown fox
            jumps over
            ˇ"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_delete_gg(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate(
        "d g g",
        indoc! {"
            The quick
            brownˇ fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d g g",
        indoc! {"
            The quick
            brown fox
            jumps over
            the lˇazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d g g",
        indoc! {"
            The qˇuick
            brown fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
    cx.simulate(
        "d g g",
        indoc! {"
            ˇ
            brown fox
            jumps over
            the lazy"},
    )
    .await
    .assert_matches();
}

#[gpui::test]
async fn test_cancel_delete_operator(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state(
        indoc! {"
                The quick brown
                fox juˇmps over
                the lazy dog"},
        Mode::Normal,
    );

    // Canceling operator twice reverts to normal mode with no active operator
    cx.simulate_keystrokes("d escape k");
    assert_eq!(cx.active_operator(), None);
    assert_eq!(cx.mode(), Mode::Normal);
    cx.assert_editor_state(indoc! {"
            The quˇick brown
            fox jumps over
            the lazy dog"});
}

#[gpui::test]
async fn test_unbound_command_cancels_pending_operator(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state(
        indoc! {"
                The quick brown
                fox juˇmps over
                the lazy dog"},
        Mode::Normal,
    );

    // Canceling operator twice reverts to normal mode with no active operator
    cx.simulate_keystrokes("d y");
    assert_eq!(cx.active_operator(), None);
    assert_eq!(cx.mode(), Mode::Normal);
}

#[gpui::test]
async fn test_delete_with_counts(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {"
                The ˇquick brown
                fox jumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d 2 d").await;
    cx.shared_state().await.assert_eq(indoc! {"
    the ˇlazy dog"});

    cx.set_shared_state(indoc! {"
                The ˇquick brown
                fox jumps over
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("2 d d").await;
    cx.shared_state().await.assert_eq(indoc! {"
    the ˇlazy dog"});

    cx.set_shared_state(indoc! {"
                The ˇquick brown
                fox jumps over
                the moon,
                a star, and
                the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("2 d 2 d").await;
    cx.shared_state().await.assert_eq(indoc! {"
    the ˇlazy dog"});
}

#[gpui::test]
async fn test_delete_to_adjacent_character(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.simulate("d t x", "ˇax").await.assert_matches();
    cx.simulate("d t x", "aˇx").await.assert_matches();
}

#[gpui::test]
async fn test_delete_sentence(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    // cx.simulate(
    //     "d )",
    //     indoc! {"
    //     Fiˇrst. Second. Third.
    //     Fourth.
    //     "},
    // )
    // .await
    // .assert_matches();

    // cx.simulate(
    //     "d )",
    //     indoc! {"
    //     First. Secˇond. Third.
    //     Fourth.
    //     "},
    // )
    // .await
    // .assert_matches();

    // // Two deletes
    // cx.simulate(
    //     "d ) d )",
    //     indoc! {"
    //     First. Second. Thirˇd.
    //     Fourth.
    //     "},
    // )
    // .await
    // .assert_matches();

    // Should delete whole line if done on first column
    cx.simulate(
        "d )",
        indoc! {"
            ˇFirst.
            Fourth.
            "},
    )
    .await
    .assert_matches();

    // Backwards it should also delete the whole first line
    cx.simulate(
        "d (",
        indoc! {"
            First.
            ˇSecond.
            Fourth.
            "},
    )
    .await
    .assert_matches();
}

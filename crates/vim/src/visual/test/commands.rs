use super::*;

#[gpui::test]
async fn test_visual_shift_d(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The ˇquick brown
        fox jumps over
        the lazy dog
        "
    })
    .await;
    cx.simulate_shared_keystrokes("v down shift-d").await;
    cx.shared_state().await.assert_eq(indoc! {
        "the ˇlazy dog\n"
    });

    cx.set_shared_state(indoc! {
        "The ˇquick brown
        fox jumps over
        the lazy dog
        "
    })
    .await;
    cx.simulate_shared_keystrokes("ctrl-v down shift-d").await;
    cx.shared_state().await.assert_eq(indoc! {
        "Theˇ•
        fox•
        the lazy dog
        "
    });
}

#[gpui::test]
async fn test_shift_y(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The ˇquick brown\n"
    })
    .await;
    cx.simulate_shared_keystrokes("v i w shift-y").await;
    cx.shared_clipboard().await.assert_eq(indoc! {
        "The quick brown\n"
    });
}

#[gpui::test]
async fn test_gv(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The ˇquick brown"
    })
    .await;
    cx.simulate_shared_keystrokes("v i w escape g v").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «quickˇ» brown"
    });

    cx.simulate_shared_keystrokes("o escape g v").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «ˇquick» brown"
    });

    cx.simulate_shared_keystrokes("escape ^ ctrl-v l").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«Thˇ»e quick brown"
    });
    cx.simulate_shared_keystrokes("g v").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The «ˇquick» brown"
    });
    cx.simulate_shared_keystrokes("g v").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«Thˇ»e quick brown"
    });

    cx.set_state(
        indoc! {"
        fiˇsh one
        fish two
        fish red
        fish blue
    "},
        Mode::Normal,
    );
    cx.simulate_keystrokes("4 g l escape escape g v");
    cx.assert_state(
        indoc! {"
            «fishˇ» one
            «fishˇ» two
            «fishˇ» red
            «fishˇ» blue
        "},
        Mode::Visual,
    );
    cx.simulate_keystrokes("y g v");
    cx.assert_state(
        indoc! {"
            «fishˇ» one
            «fishˇ» two
            «fishˇ» red
            «fishˇ» blue
        "},
        Mode::Visual,
    );
}

#[gpui::test]
async fn test_p_g_v_y(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The
        quicˇk
        brown
        fox"
    })
    .await;
    cx.simulate_shared_keystrokes("y y j shift-v p g v y").await;
    cx.shared_state().await.assert_eq(indoc! {
        "The
        quick
        ˇquick
        fox"
    });
    cx.shared_clipboard().await.assert_eq("quick\n");
}

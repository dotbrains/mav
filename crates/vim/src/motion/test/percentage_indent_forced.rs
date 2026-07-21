use super::*;

#[gpui::test]
async fn test_go_to_percentage(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    // Normal mode
    cx.set_shared_state(indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("2 0 %").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox ˇjumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog"});

    cx.simulate_shared_keystrokes("2 5 %").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox jumps over
            the ˇlazy dog
            The quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog"});

    cx.simulate_shared_keystrokes("7 5 %").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog
            The ˇquick brown
            fox jumps over
            the lazy dog"});

    // Visual mode
    cx.set_shared_state(indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v 5 0 %").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The «quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jˇ»umps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog"});

    cx.set_shared_state(indoc! {"
            The ˇquick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("v 1 0 0 %").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The «quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lazy dog
            The quick brown
            fox jumps over
            the lˇ»azy dog"});
}

#[gpui::test]
async fn test_space_non_ascii(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("ˇπππππ").await;
    cx.simulate_shared_keystrokes("3 space").await;
    cx.shared_state().await.assert_eq("πππˇππ");
}

#[gpui::test]
async fn test_space_non_ascii_eol(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            ππππˇπ
            πanotherline"})
        .await;
    cx.simulate_shared_keystrokes("4 space").await;
    cx.shared_state().await.assert_eq(indoc! {"
            πππππ
            πanˇotherline"});
}

#[gpui::test]
async fn test_backspace_non_ascii_bol(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
                        ππππ
                        πanˇotherline"})
        .await;
    cx.simulate_shared_keystrokes("4 backspace").await;
    cx.shared_state().await.assert_eq(indoc! {"
                        πππˇπ
                        πanotherline"});
}

#[gpui::test]
async fn test_go_to_indent(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state(
        indoc! {
            "func empty(a string) bool {
                     ˇif a == \"\" {
                         return true
                     }
                     return false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("[ -");
    cx.assert_state(
        indoc! {
            "ˇfunc empty(a string) bool {
                     if a == \"\" {
                         return true
                     }
                     return false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("] =");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         return true
                     }
                     return false
                ˇ}"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("[ +");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         return true
                     }
                     ˇreturn false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("2 [ =");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     ˇif a == \"\" {
                         return true
                     }
                     return false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("] +");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         ˇreturn true
                     }
                     return false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("] -");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         return true
                     ˇ}
                     return false
                }"
        },
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_delete_key_can_remove_last_character(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("abˇc").await;
    cx.simulate_shared_keystrokes("delete").await;
    cx.shared_state().await.assert_eq("aˇb");
}

#[gpui::test]
async fn test_forced_motion_delete_to_start_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
             ˇthe quick brown fox
             jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v 0").await;
    cx.shared_state().await.assert_eq(indoc! {"
             ˇhe quick brown fox
             jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
            the quick bˇrown fox
            jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v 0").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇown fox
            jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
            the quick brown foˇx
            jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v 0").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇ
            jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());
}

#[gpui::test]
async fn test_forced_motion_delete_to_middle_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
             ˇthe quick brown fox
             jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v g shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
             ˇbrown fox
             jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
            the quick bˇrown fox
            jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v g shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
            the quickˇown fox
            jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
            the quick brown foˇx
            jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v g shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
            the quicˇk
            jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
            ˇthe quick brown fox
            jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v 7 5 g shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇ fox
            jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
            ˇthe quick brown fox
            jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v 2 3 g shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
            ˇuick brown fox
            jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());
}

#[gpui::test]
async fn test_forced_motion_delete_to_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
             the quick brown foˇx
             jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v $").await;
    cx.shared_state().await.assert_eq(indoc! {"
             the quick brown foˇx
             jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
             ˇthe quick brown fox
             jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v $").await;
    cx.shared_state().await.assert_eq(indoc! {"
             ˇx
             jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());
}

#[gpui::test]
async fn test_forced_motion_yank(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
               ˇthe quick brown fox
               jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("y v j p").await;
    cx.shared_state().await.assert_eq(indoc! {"
               the quick brown fox
               ˇthe quick brown fox
               jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
              the quick bˇrown fox
              jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("y v j p").await;
    cx.shared_state().await.assert_eq(indoc! {"
              the quick brˇrown fox
              jumped overown fox
              jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
             the quick brown foˇx
             jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("y v j p").await;
    cx.shared_state().await.assert_eq(indoc! {"
             the quick brown foxˇx
             jumped over the la
             jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
             the quick brown fox
             jˇumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("y v k p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            thˇhe quick brown fox
            je quick brown fox
            jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());
}

#[gpui::test]
async fn test_inclusive_to_exclusive_delete(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
              ˇthe quick brown fox
              jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v e").await;
    cx.shared_state().await.assert_eq(indoc! {"
              ˇe quick brown fox
              jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
              the quick bˇrown fox
              jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v e").await;
    cx.shared_state().await.assert_eq(indoc! {"
              the quick bˇn fox
              jumped over the lazy dog"});
    assert!(!cx.cx.forced_motion());

    cx.set_shared_state(indoc! {"
             the quick brown foˇx
             jumped over the lazy dog"})
        .await;
    cx.simulate_shared_keystrokes("d v e").await;
    cx.shared_state().await.assert_eq(indoc! {"
    the quick brown foˇd over the lazy dog"});
    assert!(!cx.cx.forced_motion());
}

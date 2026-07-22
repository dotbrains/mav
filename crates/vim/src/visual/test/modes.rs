use super::*;

#[gpui::test]
async fn test_enter_visual_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The ˇquick brown
        fox jumps over
        the lazy dog"
    })
    .await;
    let cursor = cx.update_editor(|editor, _, cx| editor.pixel_position_of_cursor(cx));

    // entering visual mode should select the character
    // under cursor
    cx.simulate_shared_keystrokes("v").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! { "The «qˇ»uick brown
        fox jumps over
        the lazy dog"});
    cx.update_editor(|editor, _, cx| assert_eq!(cursor, editor.pixel_position_of_cursor(cx)));

    // forwards motions should extend the selection
    cx.simulate_shared_keystrokes("w j").await;
    cx.shared_state().await.assert_eq(indoc! { "The «quick brown
        fox jumps oˇ»ver
        the lazy dog"});

    cx.simulate_shared_keystrokes("escape").await;
    cx.shared_state().await.assert_eq(indoc! { "The quick brown
        fox jumps ˇover
        the lazy dog"});

    // motions work backwards
    cx.simulate_shared_keystrokes("v k b").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! { "The «ˇquick brown
        fox jumps o»ver
        the lazy dog"});

    // works on empty lines
    cx.set_shared_state(indoc! {"
        a
        ˇ
        b
        "})
        .await;
    let cursor = cx.update_editor(|editor, _, cx| editor.pixel_position_of_cursor(cx));
    cx.simulate_shared_keystrokes("v").await;
    cx.shared_state().await.assert_eq(indoc! {"
        a
        «
        ˇ»b
    "});
    cx.update_editor(|editor, _, cx| assert_eq!(cursor, editor.pixel_position_of_cursor(cx)));

    // toggles off again
    cx.simulate_shared_keystrokes("v").await;
    cx.shared_state().await.assert_eq(indoc! {"
        a
        ˇ
        b
        "});

    // works at the end of a document
    cx.set_shared_state(indoc! {"
        a
        b
        ˇ"})
        .await;

    cx.simulate_shared_keystrokes("v").await;
    cx.shared_state().await.assert_eq(indoc! {"
        a
        b
        ˇ"});
}

#[gpui::test]
async fn test_visual_insert_first_non_whitespace(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {
            "«The quick brown
            fox jumps over
            the lazy dogˇ»"
        },
        Mode::Visual,
    );
    cx.simulate_keystrokes("g shift-i");
    cx.assert_state(
        indoc! {
            "ˇThe quick brown
            ˇfox jumps over
            ˇthe lazy dog"
        },
        Mode::Insert,
    );
}

#[gpui::test]
async fn test_visual_insert_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {
            "«The quick brown
            fox jumps over
            the lazy dogˇ»"
        },
        Mode::Visual,
    );
    cx.simulate_keystrokes("g shift-a");
    cx.assert_state(
        indoc! {
            "The quick brownˇ
            fox jumps overˇ
            the lazy dogˇ"
        },
        Mode::Insert,
    );
}

#[gpui::test]
async fn test_enter_visual_line_mode(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "The ˇquick brown
        fox jumps over
        the lazy dog"
    })
    .await;
    cx.simulate_shared_keystrokes("shift-v").await;
    cx.shared_state()
        .await
        .assert_eq(indoc! { "The «qˇ»uick brown
        fox jumps over
        the lazy dog"});
    cx.simulate_shared_keystrokes("x").await;
    cx.shared_state().await.assert_eq(indoc! { "fox ˇjumps over
    the lazy dog"});

    // it should work on empty lines
    cx.set_shared_state(indoc! {"
        a
        ˇ
        b"})
        .await;
    cx.simulate_shared_keystrokes("shift-v").await;
    cx.shared_state().await.assert_eq(indoc! {"
        a
        «
        ˇ»b"});
    cx.simulate_shared_keystrokes("x").await;
    cx.shared_state().await.assert_eq(indoc! {"
        a
        ˇb"});

    // it should work at the end of the document
    cx.set_shared_state(indoc! {"
        a
        b
        ˇ"})
        .await;
    let cursor = cx.update_editor(|editor, _, cx| editor.pixel_position_of_cursor(cx));
    cx.simulate_shared_keystrokes("shift-v").await;
    cx.shared_state().await.assert_eq(indoc! {"
        a
        b
        ˇ"});
    cx.update_editor(|editor, _, cx| assert_eq!(cursor, editor.pixel_position_of_cursor(cx)));
    cx.simulate_shared_keystrokes("x").await;
    cx.shared_state().await.assert_eq(indoc! {"
        a
        ˇb"});
}

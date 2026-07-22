use super::*;

#[perf]
#[gpui::test]
async fn test_mouse_selection(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇone two three", Mode::Normal);

    let start_point = cx.pixel_position("one twˇo three");
    let end_point = cx.pixel_position("one ˇtwo three");

    cx.simulate_mouse_down(start_point, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end_point, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end_point, MouseButton::Left, Modifiers::none());

    cx.assert_state("one «ˇtwo» three", Mode::Visual)
}

#[gpui::test]
async fn test_mouse_drag_across_anchor_does_not_drift(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state("ˇone two three four", Mode::Normal);

    let click_pos = cx.pixel_position("one ˇtwo three four");
    let drag_left = cx.pixel_position("ˇone two three four");
    let anchor_pos = cx.pixel_position("one tˇwo three four");

    cx.simulate_mouse_down(click_pos, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    cx.simulate_mouse_move(drag_left, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.assert_state("«ˇone t»wo three four", Mode::Visual);

    cx.simulate_mouse_move(anchor_pos, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();

    cx.simulate_mouse_move(drag_left, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.assert_state("«ˇone t»wo three four", Mode::Visual);

    cx.simulate_mouse_move(anchor_pos, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_move(drag_left, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.assert_state("«ˇone t»wo three four", Mode::Visual);

    cx.simulate_mouse_up(drag_left, MouseButton::Left, Modifiers::none());
}

#[perf]
#[gpui::test]
async fn test_lowercase_marks(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("line one\nline ˇtwo\nline three").await;
    cx.simulate_shared_keystrokes("m a l ' a").await;
    cx.shared_state()
        .await
        .assert_eq("line one\nˇline two\nline three");
    cx.simulate_shared_keystrokes("` a").await;
    cx.shared_state()
        .await
        .assert_eq("line one\nline ˇtwo\nline three");

    cx.simulate_shared_keystrokes("^ d ` a").await;
    cx.shared_state()
        .await
        .assert_eq("line one\nˇtwo\nline three");
}

#[perf]
#[gpui::test]
async fn test_lt_gt_marks(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc!(
        "
        Line one
        Line two
        Line ˇthree
        Line four
        Line five
    "
    ))
    .await;

    cx.simulate_shared_keystrokes("v j escape k k").await;

    cx.simulate_shared_keystrokes("' <").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        ˇLine three
        Line four
        Line five
    "});

    cx.simulate_shared_keystrokes("` <").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line ˇthree
        Line four
        Line five
    "});

    cx.simulate_shared_keystrokes("' >").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line three
        ˇLine four
        Line five
    "
    });

    cx.simulate_shared_keystrokes("` >").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line three
        Line ˇfour
        Line five
    "
    });

    cx.simulate_shared_keystrokes("v i w o escape").await;
    cx.simulate_shared_keystrokes("` >").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line three
        Line fouˇr
        Line five
    "
    });
    cx.simulate_shared_keystrokes("` <").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line three
        Line ˇfour
        Line five
    "
    });
}

#[perf]
#[gpui::test]
async fn test_caret_mark(cx: &mut TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc!(
        "
        Line one
        Line two
        Line three
        ˇLine four
        Line five
    "
    ))
    .await;

    cx.simulate_shared_keystrokes("c w shift-s t r a i g h t space t h i n g escape j j")
        .await;

    cx.simulate_shared_keystrokes("' ^").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line three
        ˇStraight thing four
        Line five
    "
    });

    cx.simulate_shared_keystrokes("` ^").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line three
        Straight thingˇ four
        Line five
    "
    });

    cx.simulate_shared_keystrokes("k a ! escape k g i ?").await;
    cx.shared_state().await.assert_eq(indoc! {"
        Line one
        Line two
        Line three!?ˇ
        Straight thing four
        Line five
    "
    });
}

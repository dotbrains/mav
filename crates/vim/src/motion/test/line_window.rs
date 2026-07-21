use super::*;

#[gpui::test]
async fn test_comma_semicolon(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // f and F
    cx.set_shared_state("ˇone two three four").await;
    cx.simulate_shared_keystrokes("f o").await;
    cx.shared_state().await.assert_eq("one twˇo three four");
    cx.simulate_shared_keystrokes(",").await;
    cx.shared_state().await.assert_eq("ˇone two three four");
    cx.simulate_shared_keystrokes("2 ;").await;
    cx.shared_state().await.assert_eq("one two three fˇour");
    cx.simulate_shared_keystrokes("shift-f e").await;
    cx.shared_state().await.assert_eq("one two threˇe four");
    cx.simulate_shared_keystrokes("2 ;").await;
    cx.shared_state().await.assert_eq("onˇe two three four");
    cx.simulate_shared_keystrokes(",").await;
    cx.shared_state().await.assert_eq("one two thrˇee four");

    // t and T
    cx.set_shared_state("ˇone two three four").await;
    cx.simulate_shared_keystrokes("t o").await;
    cx.shared_state().await.assert_eq("one tˇwo three four");
    cx.simulate_shared_keystrokes(",").await;
    cx.shared_state().await.assert_eq("oˇne two three four");
    cx.simulate_shared_keystrokes("2 ;").await;
    cx.shared_state().await.assert_eq("one two three ˇfour");
    cx.simulate_shared_keystrokes("shift-t e").await;
    cx.shared_state().await.assert_eq("one two threeˇ four");
    cx.simulate_shared_keystrokes("3 ;").await;
    cx.shared_state().await.assert_eq("oneˇ two three four");
    cx.simulate_shared_keystrokes(",").await;
    cx.shared_state().await.assert_eq("one two thˇree four");
}

#[gpui::test]
async fn test_next_word_end_newline_last_char(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    let initial_state = indoc! {r"something(ˇfoo)"};
    cx.set_shared_state(initial_state).await;
    cx.simulate_shared_keystrokes("}").await;
    cx.shared_state().await.assert_eq("something(fooˇ)");
}

#[gpui::test]
async fn test_next_line_start(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("ˇone\n  two\nthree").await;
    cx.simulate_shared_keystrokes("enter").await;
    cx.shared_state().await.assert_eq("one\n  ˇtwo\nthree");
}

#[gpui::test]
async fn test_end_of_line_downward(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state("ˇ one\n two \nthree").await;
    cx.simulate_shared_keystrokes("g _").await;
    cx.shared_state().await.assert_eq(" onˇe\n two \nthree");

    cx.set_shared_state("ˇ one \n two \nthree").await;
    cx.simulate_shared_keystrokes("g _").await;
    cx.shared_state().await.assert_eq(" onˇe \n two \nthree");
    cx.simulate_shared_keystrokes("2 g _").await;
    cx.shared_state().await.assert_eq(" one \n twˇo \nthree");
}

#[gpui::test]
async fn test_end_of_line_with_vertical_motion(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    // test $ followed by k maintains end-of-line position
    cx.set_shared_state(indoc! {"
            The quick brown
            fˇox
            jumps over the
            lazy dog
            "})
        .await;
    cx.simulate_shared_keystrokes("$ k").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick browˇn
            fox
            jumps over the
            lazy dog
            "});
    cx.simulate_shared_keystrokes("j j").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown
            fox
            jumps over thˇe
            lazy dog
            "});

    // test horizontal movement resets the end-of-line behavior
    cx.set_shared_state(indoc! {"
            The quick brown fox
            jumps over the
            lazy ˇdog
            "})
        .await;
    cx.simulate_shared_keystrokes("$ k").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown fox
            jumps over thˇe
            lazy dog
            "});
    cx.simulate_shared_keystrokes("b b").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown fox
            jumps ˇover the
            lazy dog
            "});
    cx.simulate_shared_keystrokes("k").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quˇick brown fox
            jumps over the
            lazy dog
            "});

    // Test that, when the cursor is moved to the end of the line using `l`,
    // if `$` is used, the cursor stays at the end of the line when moving
    // to a longer line, ensuring that the selection goal was correctly
    // updated.
    cx.set_shared_state(indoc! {"
            The quick brown fox
            jumps over the
            lazy dˇog
            "})
        .await;
    cx.simulate_shared_keystrokes("l").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown fox
            jumps over the
            lazy doˇg
            "});
    cx.simulate_shared_keystrokes("$ k").await;
    cx.shared_state().await.assert_eq(indoc! {"
            The quick brown fox
            jumps over thˇe
            lazy dog
            "});
}

#[gpui::test]
async fn test_window_top(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    let initial_state = indoc! {r"abc
          def
          paragraph
          the second
          third ˇand
          final"};

    cx.set_shared_state(initial_state).await;
    cx.simulate_shared_keystrokes("shift-h").await;
    cx.shared_state().await.assert_eq(indoc! {r"abˇc
          def
          paragraph
          the second
          third and
          final"});

    // clip point
    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 6
          7 8 ˇ9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-h").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 ˇ3
          4 5 6
          7 8 9
          "});

    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 6
          ˇ7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-h").await;
    cx.shared_state().await.assert_eq(indoc! {"
          ˇ1 2 3
          4 5 6
          7 8 9
          "});

    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 ˇ6
          7 8 9"})
        .await;
    cx.simulate_shared_keystrokes("9 shift-h").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          4 5 6
          7 8 ˇ9"});
}

#[gpui::test]
async fn test_window_middle(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    let initial_state = indoc! {r"abˇc
          def
          paragraph
          the second
          third and
          final"};

    cx.set_shared_state(initial_state).await;
    cx.simulate_shared_keystrokes("shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {r"abc
          def
          paˇragraph
          the second
          third and
          final"});

    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 6
          7 8 ˇ9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          4 5 ˇ6
          7 8 9
          "});
    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 6
          ˇ7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          ˇ4 5 6
          7 8 9
          "});
    cx.set_shared_state(indoc! {r"
          ˇ1 2 3
          4 5 6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          ˇ4 5 6
          7 8 9
          "});
    cx.set_shared_state(indoc! {r"
          1 2 3
          ˇ4 5 6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          ˇ4 5 6
          7 8 9
          "});
    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 ˇ6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-m").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          4 5 ˇ6
          7 8 9
          "});
}

#[gpui::test]
async fn test_window_bottom(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    let initial_state = indoc! {r"abc
          deˇf
          paragraph
          the second
          third and
          final"};

    cx.set_shared_state(initial_state).await;
    cx.simulate_shared_keystrokes("shift-l").await;
    cx.shared_state().await.assert_eq(indoc! {r"abc
          def
          paragraph
          the second
          third and
          fiˇnal"});

    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 ˇ6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-l").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          4 5 6
          7 8 9
          ˇ"});

    cx.set_shared_state(indoc! {r"
          1 2 3
          ˇ4 5 6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-l").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          4 5 6
          7 8 9
          ˇ"});

    cx.set_shared_state(indoc! {r"
          1 2 ˇ3
          4 5 6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-l").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          4 5 6
          7 8 9
          ˇ"});

    cx.set_shared_state(indoc! {r"
          ˇ1 2 3
          4 5 6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("shift-l").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 3
          4 5 6
          7 8 9
          ˇ"});

    cx.set_shared_state(indoc! {r"
          1 2 3
          4 5 ˇ6
          7 8 9
          "})
        .await;
    cx.simulate_shared_keystrokes("9 shift-l").await;
    cx.shared_state().await.assert_eq(indoc! {"
          1 2 ˇ3
          4 5 6
          7 8 9
          "});
}

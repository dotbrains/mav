use super::*;

#[gpui::test]
async fn test_previous_word_end(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {r"
        456 5ˇ67 678
        "})
        .await;
    cx.simulate_shared_keystrokes("g e").await;
    cx.shared_state().await.assert_eq(indoc! {"
        45ˇ6 567 678
        "});

    // Test times
    cx.set_shared_state(indoc! {r"
        123 234 345
        456 5ˇ67 678
        "})
        .await;
    cx.simulate_shared_keystrokes("4 g e").await;
    cx.shared_state().await.assert_eq(indoc! {"
        12ˇ3 234 345
        456 567 678
        "});

    // With punctuation
    cx.set_shared_state(indoc! {r"
        123 234 345
        4;5.6 5ˇ67 678
        789 890 901
        "})
        .await;
    cx.simulate_shared_keystrokes("g e").await;
    cx.shared_state().await.assert_eq(indoc! {"
          123 234 345
          4;5.ˇ6 567 678
          789 890 901
        "});

    // With punctuation and count
    cx.set_shared_state(indoc! {r"
        123 234 345
        4;5.6 5ˇ67 678
        789 890 901
        "})
        .await;
    cx.simulate_shared_keystrokes("5 g e").await;
    cx.shared_state().await.assert_eq(indoc! {"
          123 234 345
          ˇ4;5.6 567 678
          789 890 901
        "});

    // newlines
    cx.set_shared_state(indoc! {r"
        123 234 345

        78ˇ9 890 901
        "})
        .await;
    cx.simulate_shared_keystrokes("g e").await;
    cx.shared_state().await.assert_eq(indoc! {"
          123 234 345
          ˇ
          789 890 901
        "});
    cx.simulate_shared_keystrokes("g e").await;
    cx.shared_state().await.assert_eq(indoc! {"
          123 234 34ˇ5

          789 890 901
        "});

    // With punctuation
    cx.set_shared_state(indoc! {r"
        123 234 345
        4;5.ˇ6 567 678
        789 890 901
        "})
        .await;
    cx.simulate_shared_keystrokes("g shift-e").await;
    cx.shared_state().await.assert_eq(indoc! {"
          123 234 34ˇ5
          4;5.6 567 678
          789 890 901
        "});

    // With multi byte char
    cx.set_shared_state(indoc! {r"
        bar ˇó
        "})
        .await;
    cx.simulate_shared_keystrokes("g e").await;
    cx.shared_state().await.assert_eq(indoc! {"
        baˇr ó
        "});
}

#[gpui::test]
async fn test_visual_match_eol(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            fn aˇ() {
              return
            }
        "})
        .await;
    cx.simulate_shared_keystrokes("v $ %").await;
    cx.shared_state().await.assert_eq(indoc! {"
            fn a«() {
              return
            }ˇ»
        "});
}

#[gpui::test]
async fn test_clipping_with_inlay_hints(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
                struct Foo {
                ˇ
                }
            "},
        Mode::Normal,
    );

    cx.update_editor(|editor, _window, cx| {
        let range = editor.selections.newest_anchor().range();
        let inlay_text = "  field: int,\n  field2: string\n  field3: float";
        let inlay = Inlay::edit_prediction(1, range.start, inlay_text);
        editor.splice_inlays(&[], vec![inlay], cx);
    });

    cx.simulate_keystrokes("j");
    cx.assert_state(
        indoc! {"
                struct Foo {

                ˇ}
            "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_clipping_with_inlay_hints_end_of_line(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {"
            ˇstruct Foo {

            }
        "},
        Mode::Normal,
    );
    cx.update_editor(|editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let end_of_line =
            snapshot.anchor_after(Point::new(0, snapshot.line_len(MultiBufferRow(0))));
        let inlay_text = " hint";
        let inlay = Inlay::edit_prediction(1, end_of_line, inlay_text);
        editor.splice_inlays(&[], vec![inlay], cx);
    });
    cx.simulate_keystrokes("$");
    cx.assert_state(
        indoc! {"
            struct Foo ˇ{

            }
        "},
        Mode::Normal,
    );
}

#[gpui::test]
async fn test_visual_mode_with_inlay_hints_on_empty_line(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Test the exact scenario from issue #29134
    cx.set_state(
        indoc! {"
                fn main() {
                    let this_is_a_long_name = Vec::<u32>::new();
                    let new_oneˇ = this_is_a_long_name
                        .iter()
                        .map(|i| i + 1)
                        .map(|i| i * 2)
                        .collect::<Vec<_>>();
                }
            "},
        Mode::Normal,
    );

    // Add type hint inlay on the empty line (line 3, after "this_is_a_long_name")
    cx.update_editor(|editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        // The empty line is at line 3 (0-indexed)
        let line_start = snapshot.anchor_after(Point::new(3, 0));
        let inlay_text = ": Vec<u32>";
        let inlay = Inlay::edit_prediction(1, line_start, inlay_text);
        editor.splice_inlays(&[], vec![inlay], cx);
    });

    // Enter visual mode
    cx.simulate_keystrokes("v");
    cx.assert_state(
        indoc! {"
                fn main() {
                    let this_is_a_long_name = Vec::<u32>::new();
                    let new_one« ˇ»= this_is_a_long_name
                        .iter()
                        .map(|i| i + 1)
                        .map(|i| i * 2)
                        .collect::<Vec<_>>();
                }
            "},
        Mode::Visual,
    );

    // Move down - should go to the beginning of line 4, not skip to line 5
    cx.simulate_keystrokes("j");
    cx.assert_state(
        indoc! {"
                fn main() {
                    let this_is_a_long_name = Vec::<u32>::new();
                    let new_one« = this_is_a_long_name
                      ˇ»  .iter()
                        .map(|i| i + 1)
                        .map(|i| i * 2)
                        .collect::<Vec<_>>();
                }
            "},
        Mode::Visual,
    );

    // Test with multiple movements
    cx.set_state("let aˇ = 1;\nlet b = 2;\n\nlet c = 3;", Mode::Normal);

    // Add type hint on the empty line
    cx.update_editor(|editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let empty_line_start = snapshot.anchor_after(Point::new(2, 0));
        let inlay_text = ": i32";
        let inlay = Inlay::edit_prediction(2, empty_line_start, inlay_text);
        editor.splice_inlays(&[], vec![inlay], cx);
    });

    // Enter visual mode and move down twice
    cx.simulate_keystrokes("v j j");
    cx.assert_state("let a« = 1;\nlet b = 2;\n\nˇ»let c = 3;", Mode::Visual);
}

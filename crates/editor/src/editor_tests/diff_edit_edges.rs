use super::*;

#[gpui::test]
async fn test_toggling_adjacent_diff_hunks_2(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        lineA
        lineB
        lineC
        lineD
        "#
    .unindent();

    cx.set_state(
        &r#"
        ˇlineA1
        lineB
        lineD
        "#
        .unindent(),
    );
    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - lineA
        + ˇlineA1
          lineB
          lineD
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
        editor.move_right(&MoveRight, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - lineA
        + lineA1
          lˇineB
        - lineC
          lineD
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_edits_around_expanded_deletion_hunks(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
    "#
    .unindent();
    executor.run_until_parked();
    cx.set_state(
        &r#"
        use some::mod1;
        use some::mod2;

        ˇconst B: u32 = 42;
        const C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
        ˇconst B: u32 = 42;
        const C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
      - const B: u32 = 42;
        ˇconst C: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
      - const B: u32 = 42;
      - const C: u32 = 42;
        ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("replacement", window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

      - const A: u32 = 42;
      - const B: u32 = 42;
      - const C: u32 = 42;
      -
      + replacementˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );
}

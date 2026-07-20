use super::*;

#[gpui::test]
async fn test_backspace_after_deletion_hunk(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let base_text = r#"
        one
        two
        three
        four
        five
    "#
    .unindent();
    executor.run_until_parked();
    cx.set_state(
        &r#"
        one
        two
        fˇour
        five
        "#
        .unindent(),
    );

    cx.set_head_text(&base_text);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&ExpandAllDiffHunks, window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
          one
          two
        - three
          fˇour
          five
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Backspace, window, cx);
        editor.backspace(&Backspace, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
          one
          two
        - threeˇ
        - four
        + our
          five
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_edit_after_expanded_modification_hunk(
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
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
    .unindent();

    cx.set_state(
        &r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 43ˇ
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
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

        const A: u32 = 42;
        const B: u32 = 42;
      - const C: u32 = 42;
      + const C: u32 = 43ˇ
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("\nnew_line\n", window, cx);
    });
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
        const B: u32 = 42;
      - const C: u32 = 42;
      + const C: u32 = 43
      + new_line
      + ˇ
        const D: u32 = 42;


        fn main() {
            println!("hello");

            println!("world");
        }"#
        .unindent(),
    );
}

#[gpui::test]
async fn test_stage_and_unstage_added_file_hunk(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_editor(|editor, _, cx| {
        editor.set_expand_all_diff_hunks(cx);
    });

    let working_copy = r#"
            ˇfn main() {
                println!("hello, world!");
            }
        "#
    .unindent();

    cx.set_state(&working_copy);
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
            + ˇfn main() {
            +     println!("hello, world!");
            + }
        "#
        .unindent(),
    );
    cx.assert_index_text(None);

    cx.update_editor(|editor, window, cx| {
        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    executor.run_until_parked();
    cx.assert_index_text(Some(&working_copy.replace("ˇ", "")));
    cx.assert_state_with_diff(
        r#"
            + ˇfn main() {
            +     println!("hello, world!");
            + }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.toggle_staged_selected_diff_hunks(&Default::default(), window, cx);
    });
    executor.run_until_parked();
    cx.assert_index_text(None);
}

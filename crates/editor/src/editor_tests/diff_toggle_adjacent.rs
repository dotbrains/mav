use super::*;

#[gpui::test]
async fn test_expand_diff_hunk_at_excerpt_boundary(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let base = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\n";
    let text = "aaa\nBBB\nBB2\nccc\nDDD\nEEE\nfff\nggg\nhhh\niii\n";

    let buffer = cx.new(|cx| Buffer::local(text.to_string(), cx));
    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::new(0, 0)..Point::new(1, 3),
                Point::new(4, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(9, 3),
            ],
            0,
            cx,
        );
        assert_eq!(multibuffer.read(cx).excerpts().count(), 3);
        multibuffer
    });

    let editor =
        cx.add_window(|window, cx| Editor::new(EditorMode::full(), multi_buffer, None, window, cx));
    editor
        .update(cx, |editor, _window, cx| {
            let diff = cx.new(|cx| {
                BufferDiff::new_with_base_text(base, &buffer.read(cx).text_snapshot(), cx)
            });
            editor
                .buffer
                .update(cx, |buffer, cx| buffer.add_diff(diff, cx))
        })
        .unwrap();

    let mut cx = EditorTestContext::for_editor(editor, cx).await;
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&Default::default(), window, cx)
    });
    cx.executor().run_until_parked();

    // When the start of a hunk coincides with the start of its excerpt,
    // the hunk is expanded. When the start of a hunk is earlier than
    // the start of its excerpt, the hunk is not expanded.
    cx.assert_state_with_diff(
        "
            ˇaaa
          - bbb
          + BBB
          - ddd
          - eee
          + DDD
          + EEE
            fff
            iii"
        .unindent(),
    );
}

#[gpui::test]
async fn test_edits_around_expanded_insertion_hunks(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;

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

        const A: u32 = 42;
        const B: u32 = 42;
        const C: u32 = 42;
        ˇ

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

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| editor.handle_input("const D: u32 = 42;\n", window, cx));
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + const D: u32 = 42;
      + ˇ

        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| editor.handle_input("const E: u32 = 42;\n", window, cx));
    executor.run_until_parked();

    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + const D: u32 = 42;
      + const E: u32 = 42;
      + ˇ

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

        const A: u32 = 42;
      + const B: u32 = 42;
      + const C: u32 = 42;
      + const D: u32 = 42;
      + const E: u32 = 42;
        ˇ
        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_up(&MoveUp, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        use some::mod1;
        use some::mod2;

        const A: u32 = 42;
      + const B: u32 = 42;
        ˇ
        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.select_up_by_lines(&SelectUpByLines { lines: 5 }, window, cx);
        editor.delete_line(&DeleteLine, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        ˇ
        fn main() {
            println!("hello");

            println!("world");
        }
      "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_toggling_adjacent_diff_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_head_text(indoc! { "
        one
        two
        three
        four
        five
        "
    });
    cx.set_state(indoc! { "
        one
        ˇthree
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
        one
      - two
        ˇthree
      - four
        five
    "}
        .to_string(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });

    cx.assert_state_with_diff(
        indoc! { "
        one
        ˇthree
        five
    "}
        .to_string(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_up(&MoveUp, window, cx);
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
        ˇone
      - two
        three
        five
    "}
        .to_string(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
        editor.move_down(&MoveDown, window, cx);
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
        one
      - two
        ˇthree
      - four
        five
    "}
        .to_string(),
    );

    cx.set_state(indoc! { "
        one
        ˇTWO
        three
        four
        five
    "});
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });

    cx.assert_state_with_diff(
        indoc! { "
            one
          - two
          + ˇTWO
            three
            four
            five
        "}
        .to_string(),
    );
    cx.update_editor(|editor, window, cx| {
        editor.move_up(&Default::default(), window, cx);
        editor.toggle_selected_diff_hunks(&Default::default(), window, cx);
    });
    cx.assert_state_with_diff(
        indoc! { "
            one
            ˇTWO
            three
            four
            five
        "}
        .to_string(),
    );
}

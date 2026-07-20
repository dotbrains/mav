use super::*;

#[gpui::test]
async fn test_toggle_selected_diff_hunks(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        use some::mod;

        const A: u32 = 42;

        fn main() {
            println!("hello");

            println!("world");
        }
        "#
    .unindent();

    cx.set_state(
        &r#"
        use some::modified;

        ˇ
        fn main() {
            println!("hello there");

            println!("around the");
            println!("world");
        }
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_hunk(&GoToHunk, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
          use some::modified;


          fn main() {
        -     println!("hello");
        + ˇ    println!("hello there");

              println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        for _ in 0..2 {
            editor.go_to_next_hunk(&GoToHunk, window, cx);
            editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
        }
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - use some::mod;
        + ˇuse some::modified;


          fn main() {
        -     println!("hello");
        +     println!("hello there");

        +     println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_hunk(&GoToHunk, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - use some::mod;
        + use some::modified;

        - const A: u32 = 42;
          ˇ
          fn main() {
        -     println!("hello");
        +     println!("hello there");

        +     println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.cancel(&Cancel, window, cx);
    });

    cx.assert_state_with_diff(
        r#"
          use some::modified;

          ˇ
          fn main() {
              println!("hello there");

              println!("around the");
              println!("world");
          }
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_diff_base_change_with_expanded_diff_hunks(
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

    cx.set_state(
        &r#"
        use some::mod2;

        const A: u32 = 42;
        const C: u32 = 42;

        fn main(ˇ) {
            //println!("hello");

            println!("world");
            //
            //
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
        - use some::mod1;
          use some::mod2;

          const A: u32 = 42;
        - const B: u32 = 42;
          const C: u32 = 42;

          fn main(ˇ) {
        -     println!("hello");
        +     //println!("hello");

              println!("world");
        +     //
        +     //
          }
        "#
        .unindent(),
    );

    cx.set_head_text("new diff base!");
    executor.run_until_parked();
    cx.assert_state_with_diff(
        r#"
        - new diff base!
        + use some::mod2;
        +
        + const A: u32 = 42;
        + const C: u32 = 42;
        +
        + fn main(ˇ) {
        +     //println!("hello");
        +
        +     println!("world");
        +     //
        +     //
        + }
        "#
        .unindent(),
    );
}

#[gpui::test]
async fn test_toggle_diff_expand_in_multi_buffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let file_1_old = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj";
    let file_1_new = "aaa\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj";
    let file_2_old = "lll\nmmm\nnnn\nooo\nppp\nqqq\nrrr\nsss\nttt\nuuu";
    let file_2_new = "lll\nmmm\nNNN\nooo\nppp\nqqq\nrrr\nsss\nttt\nuuu";
    let file_3_old = "111\n222\n333\n444\n555\n777\n888\n999\n000\n!!!";
    let file_3_new = "111\n222\n333\n444\n555\n666\n777\n888\n999\n000\n!!!";

    let buffer_1 = cx.new(|cx| Buffer::local(file_1_new.to_string(), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(file_2_new.to_string(), cx));
    let buffer_3 = cx.new(|cx| Buffer::local(file_3_new.to_string(), cx));

    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [
                Point::new(0, 0)..Point::new(2, 3),
                Point::new(5, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(10, 3),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [
                Point::new(0, 0)..Point::new(2, 3),
                Point::new(5, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(10, 3),
            ],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer_3.clone(),
            [
                Point::new(0, 0)..Point::new(2, 3),
                Point::new(5, 0)..Point::new(6, 3),
                Point::new(9, 0)..Point::new(10, 3),
            ],
            0,
            cx,
        );
        assert_eq!(multibuffer.read(cx).excerpts().count(), 9);
        multibuffer
    });

    let editor =
        cx.add_window(|window, cx| Editor::new(EditorMode::full(), multi_buffer, None, window, cx));
    editor
        .update(cx, |editor, _window, cx| {
            for (buffer, diff_base) in [
                (buffer_1.clone(), file_1_old),
                (buffer_2.clone(), file_2_old),
                (buffer_3.clone(), file_3_old),
            ] {
                let diff = cx.new(|cx| {
                    BufferDiff::new_with_base_text(diff_base, &buffer.read(cx).text_snapshot(), cx)
                });
                editor
                    .buffer
                    .update(cx, |buffer, cx| buffer.add_diff(diff, cx));
            }
        })
        .unwrap();

    let mut cx = EditorTestContext::for_editor(editor, cx).await;
    cx.run_until_parked();

    cx.assert_editor_state(
        &"
            ˇaaa
            ccc
            ddd
            ggg
            hhh

            lll
            mmm
            NNN
            qqq
            rrr
            uuu
            111
            222
            333
            666
            777
            000
            !!!"
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
        editor.toggle_selected_diff_hunks(&ToggleSelectedDiffHunks, window, cx);
    });
    cx.executor().run_until_parked();

    cx.assert_state_with_diff(
        "
            «aaa
          - bbb
            ccc
            ddd
            ggg
            hhh

            lll
            mmm
          - nnn
          + NNN
            qqq
            rrr
            uuu
            111
            222
            333
          + 666
            777
            000
            !!!ˇ»"
            .unindent(),
    );
}

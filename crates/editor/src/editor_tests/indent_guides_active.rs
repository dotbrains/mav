use super::*;

#[gpui::test]
async fn test_indent_guide_without_brackets(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        block1
            block2
                block3
                    block4
            block2
        block1
        block1"
            .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        1..10,
        vec![
            indent_guide(buffer_id, 1, 4, 0),
            indent_guide(buffer_id, 2, 3, 1),
            indent_guide(buffer_id, 3, 3, 2),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_ends_before_empty_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        block1
            block2
                block3

        block1
        block1"
            .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..6,
        vec![
            indent_guide(buffer_id, 1, 2, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_ignored_only_whitespace_lines(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        function component() {
        \treturn (
        \t\t\t
        \t\t<div>
        \t\t\t<abc></abc>
        \t\t</div>
        \t)
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..8,
        vec![
            indent_guide(buffer_id, 1, 6, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 4, 4, 2),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_fallback_to_next_non_entirely_whitespace_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        function component() {
        \treturn (
        \t
        \t\t<div>
        \t\t\t<abc></abc>
        \t\t</div>
        \t)
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..8,
        vec![
            indent_guide(buffer_id, 1, 6, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 4, 4, 2),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_continuing_off_screen(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        block1



            block2
        "
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..1, vec![indent_guide(buffer_id, 1, 1, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_tabs(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        def a:
        \tb = 3
        \tif True:
        \t\tc = 4
        \t\td = 5
        \tprint(b)
        "
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..6,
        vec![
            indent_guide(buffer_id, 1, 5, 0),
            indent_guide(buffer_id, 3, 4, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_single_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    fn main() {
        let a = 1;
    }"
        .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(1, 0)])
        });
    });

    assert_indent_guides(
        0..3,
        vec![indent_guide(buffer_id, 1, 1, 0)],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_respect_indented_range(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    fn main() {
        if 1 == 2 {
            let a = 1;
        }
    }"
        .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(1, 0)])
        });
    });
    cx.run_until_parked();

    assert_indent_guides(
        0..4,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        Some(vec![1]),
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(2, 0)..Point::new(2, 0)])
        });
    });
    cx.run_until_parked();

    assert_indent_guides(
        0..4,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        Some(vec![1]),
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(3, 0)..Point::new(3, 0)])
        });
    });
    cx.run_until_parked();

    assert_indent_guides(
        0..4,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_empty_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    fn main() {
        let a = 1;

        let b = 2;
    }"
        .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(2, 0)..Point::new(2, 0)])
        });
    });

    assert_indent_guides(
        0..5,
        vec![indent_guide(buffer_id, 1, 3, 0)],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_active_indent_guide_non_matching_indent(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
    def m:
        a = 1
        pass"
            .unindent(),
        cx,
    )
    .await;

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(1, 0)])
        });
    });

    assert_indent_guides(
        0..3,
        vec![indent_guide(buffer_id, 1, 2, 0)],
        Some(vec![0]),
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_with_expanded_diff_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;
    let text = indoc! {
        "
        impl A {
            fn b() {
                0;
                3;
                5;
                6;
                7;
            }
        }
        "
    };
    let base_text = indoc! {
        "
        impl A {
            fn b() {
                0;
                1;
                2;
                3;
                4;
            }
            fn c() {
                5;
                6;
                7;
            }
        }
        "
    };

    cx.update_editor(|editor, window, cx| {
        editor.set_text(text, window, cx);

        editor.buffer().update(cx, |multibuffer, cx| {
            let buffer = multibuffer.as_singleton().unwrap();
            let diff = cx.new(|cx| {
                BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx)
            });

            multibuffer.set_all_diff_hunks_expanded(cx);
            multibuffer.add_diff(diff, cx);

            buffer.read(cx).remote_id()
        })
    });
    cx.run_until_parked();

    cx.assert_state_with_diff(
        indoc! { "
          impl A {
              fn b() {
                  0;
        -         1;
        -         2;
                  3;
        -         4;
        -     }
        -     fn c() {
                  5;
                  6;
                  7;
              }
          }
          ˇ"
        }
        .to_string(),
    );

    let mut actual_guides = cx.update_editor(|editor, window, cx| {
        editor
            .snapshot(window, cx)
            .buffer_snapshot()
            .indent_guides_in_range(Anchor::Min..Anchor::Max, false, cx)
            .map(|guide| (guide.start_row..=guide.end_row, guide.depth))
            .collect::<Vec<_>>()
    });
    actual_guides.sort_by_key(|item| (*item.0.start(), item.1));
    assert_eq!(
        actual_guides,
        vec![
            (MultiBufferRow(1)..=MultiBufferRow(12), 0),
            (MultiBufferRow(2)..=MultiBufferRow(6), 1),
            (MultiBufferRow(9)..=MultiBufferRow(11), 1),
        ]
    );
}

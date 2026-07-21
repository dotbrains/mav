use super::*;

#[gpui::test]
fn test_layout_line_numbers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(6, 6, 'a'), cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });

    let editor = window.root(cx).unwrap();
    let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
    let line_height = window
        .update(cx, |_, window, _| {
            style.text.line_height_in_pixels(window.rem_size())
        })
        .unwrap();
    let element = EditorElement::new(&editor, style);
    let snapshot = window
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();

    let layouts = cx
        .update_window(*window, |_, window, cx| {
            element.layout_line_numbers(
                &test_gutter(line_height, &snapshot),
                &BTreeMap::default(),
                Some(DisplayRow(0)),
                window,
                cx,
            )
        })
        .unwrap();
    assert_eq!(layouts.len(), 6);

    let relative_rows = window
        .update(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            snapshot.calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(6)),
                DisplayRow(3),
                false,
            )
        })
        .unwrap();
    assert_eq!(relative_rows[&DisplayRow(0)], 3);
    assert_eq!(relative_rows[&DisplayRow(1)], 2);
    assert_eq!(relative_rows[&DisplayRow(2)], 1);
    // current line has no relative number
    assert!(!relative_rows.contains_key(&DisplayRow(3)));
    assert_eq!(relative_rows[&DisplayRow(4)], 1);
    assert_eq!(relative_rows[&DisplayRow(5)], 2);

    // works if cursor is before screen
    let relative_rows = window
        .update(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            snapshot.calculate_relative_line_numbers(
                &(DisplayRow(3)..DisplayRow(6)),
                DisplayRow(1),
                false,
            )
        })
        .unwrap();
    assert_eq!(relative_rows.len(), 3);
    assert_eq!(relative_rows[&DisplayRow(3)], 2);
    assert_eq!(relative_rows[&DisplayRow(4)], 3);
    assert_eq!(relative_rows[&DisplayRow(5)], 4);

    // works if cursor is after screen
    let relative_rows = window
        .update(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            snapshot.calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(3)),
                DisplayRow(6),
                false,
            )
        })
        .unwrap();
    assert_eq!(relative_rows.len(), 3);
    assert_eq!(relative_rows[&DisplayRow(0)], 5);
    assert_eq!(relative_rows[&DisplayRow(1)], 4);
    assert_eq!(relative_rows[&DisplayRow(2)], 3);

    let gutter = Gutter {
        row_infos: &(0..6)
            .map(|row| RowInfo {
                buffer_row: Some(row),
                diff_status: (row == DELETED_LINE).then(|| {
                    DiffHunkStatus::deleted(buffer_diff::DiffHunkSecondaryStatus::NoSecondaryHunk)
                }),
                ..Default::default()
            })
            .collect::<Vec<_>>(),
        ..test_gutter(line_height, &snapshot)
    };

    const DELETED_LINE: u32 = 3;
    let layouts = cx
        .update_window(*window, |_, window, cx| {
            element.layout_line_numbers(
                &gutter,
                &BTreeMap::default(),
                Some(DisplayRow(0)),
                window,
                cx,
            )
        })
        .unwrap();
    assert_eq!(layouts.len(), 5,);
    assert!(
        layouts.get(&MultiBufferRow(DELETED_LINE)).is_none(),
        "Deleted line should not have a line number"
    );
}

#[gpui::test]
async fn test_layout_line_numbers_with_folded_lines(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let python_lang = languages::language("python", tree_sitter_python::LANGUAGE.into());

    let window = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| {
            Buffer::local(
                indoc::indoc! {"
                    fn test() -> int {
                        return 2;
                    }

                    fn another_test() -> int {
                        # This is a very peculiar method that is hard to grasp.
                        return 4;
                    }
                "},
                cx,
            )
            .with_language(python_lang, cx)
        });

        let buffer = MultiBuffer::build_from_buffer(buffer, cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });

    let editor = window.root(cx).unwrap();
    let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
    let line_height = window
        .update(cx, |_, window, _| {
            style.text.line_height_in_pixels(window.rem_size())
        })
        .unwrap();
    let element = EditorElement::new(&editor, style);
    let snapshot = window
        .update(cx, |editor, window, cx| {
            editor.fold_at(MultiBufferRow(0), window, cx);
            editor.snapshot(window, cx)
        })
        .unwrap();

    let layouts = cx
        .update_window(*window, |_, window, cx| {
            element.layout_line_numbers(
                &test_gutter(line_height, &snapshot),
                &BTreeMap::default(),
                Some(DisplayRow(3)),
                window,
                cx,
            )
        })
        .unwrap();
    assert_eq!(layouts.len(), 6);

    let relative_rows = window
        .update(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            snapshot.calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(6)),
                DisplayRow(3),
                false,
            )
        })
        .unwrap();
    assert_eq!(relative_rows[&DisplayRow(0)], 3);
    assert_eq!(relative_rows[&DisplayRow(1)], 2);
    assert_eq!(relative_rows[&DisplayRow(2)], 1);
    // current line has no relative number
    assert!(!relative_rows.contains_key(&DisplayRow(3)));
    assert_eq!(relative_rows[&DisplayRow(4)], 1);
    assert_eq!(relative_rows[&DisplayRow(5)], 2);
}

#[gpui::test]
fn test_layout_line_numbers_wrapping(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(6, 6, 'a'), cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });

    update_test_language_settings(cx, &|s| {
        s.defaults.preferred_line_length = Some(5_u32);
        s.defaults.soft_wrap = Some(language_settings::SoftWrap::Bounded);
    });

    let editor = window.root(cx).unwrap();
    let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
    let line_height = window
        .update(cx, |_, window, _| {
            style.text.line_height_in_pixels(window.rem_size())
        })
        .unwrap();
    let element = EditorElement::new(&editor, style);
    let snapshot = window
        .update(cx, |editor, window, cx| editor.snapshot(window, cx))
        .unwrap();

    let layouts = cx
        .update_window(*window, |_, window, cx| {
            element.layout_line_numbers(
                &test_gutter(line_height, &snapshot),
                &BTreeMap::default(),
                Some(DisplayRow(0)),
                window,
                cx,
            )
        })
        .unwrap();
    assert_eq!(layouts.len(), 3);

    let relative_rows = window
        .update(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            snapshot.calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(6)),
                DisplayRow(3),
                true,
            )
        })
        .unwrap();

    assert_eq!(relative_rows[&DisplayRow(0)], 3);
    assert_eq!(relative_rows[&DisplayRow(1)], 2);
    assert_eq!(relative_rows[&DisplayRow(2)], 1);
    // current line has no relative number
    assert!(!relative_rows.contains_key(&DisplayRow(3)));
    assert_eq!(relative_rows[&DisplayRow(4)], 1);
    assert_eq!(relative_rows[&DisplayRow(5)], 2);

    let layouts = cx
        .update_window(*window, |_, window, cx| {
            element.layout_line_numbers(
                &Gutter {
                    row_infos: &(0..6)
                        .map(|row| RowInfo {
                            buffer_row: Some(row),
                            diff_status: Some(DiffHunkStatus::deleted(
                                buffer_diff::DiffHunkSecondaryStatus::NoSecondaryHunk,
                            )),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    ..test_gutter(line_height, &snapshot)
                },
                &BTreeMap::from_iter([(DisplayRow(0), LineHighlightSpec::default())]),
                Some(DisplayRow(0)),
                window,
                cx,
            )
        })
        .unwrap();
    assert!(
        layouts.is_empty(),
        "Deleted lines should have no line number"
    );

    let relative_rows = window
        .update(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            snapshot.calculate_relative_line_numbers(
                &(DisplayRow(0)..DisplayRow(6)),
                DisplayRow(3),
                true,
            )
        })
        .unwrap();

    // Deleted lines should still have relative numbers
    assert_eq!(relative_rows[&DisplayRow(0)], 3);
    assert_eq!(relative_rows[&DisplayRow(1)], 2);
    assert_eq!(relative_rows[&DisplayRow(2)], 1);
    // current line, even if deleted, has no relative number
    assert!(!relative_rows.contains_key(&DisplayRow(3)));
    assert_eq!(relative_rows[&DisplayRow(4)], 1);
    assert_eq!(relative_rows[&DisplayRow(5)], 2);
}

use super::*;

#[gpui::test]
fn test_all_invisibles_drawing(cx: &mut TestAppContext) {
    const TAB_SIZE: u32 = 4;

    let input_text = "\t \t|\t| a b";
    let expected_invisibles = vec![
        Invisible::Tab {
            line_start_offset: 0,
            line_end_offset: TAB_SIZE as usize,
        },
        Invisible::Whitespace {
            line_start_offset: TAB_SIZE as usize,
            line_end_offset: TAB_SIZE as usize + 1,
        },
        Invisible::Tab {
            line_start_offset: TAB_SIZE as usize + 1,
            line_end_offset: TAB_SIZE as usize * 2,
        },
        Invisible::Tab {
            line_start_offset: TAB_SIZE as usize * 2 + 1,
            line_end_offset: TAB_SIZE as usize * 3,
        },
        Invisible::Whitespace {
            line_start_offset: TAB_SIZE as usize * 3 + 1,
            line_end_offset: TAB_SIZE as usize * 3 + 2,
        },
        Invisible::Whitespace {
            line_start_offset: TAB_SIZE as usize * 3 + 3,
            line_end_offset: TAB_SIZE as usize * 3 + 4,
        },
    ];
    assert_eq!(
        expected_invisibles.len(),
        input_text
            .chars()
            .filter(|initial_char| initial_char.is_whitespace())
            .count(),
        "Hardcoded expected invisibles differ from the actual ones in '{input_text}'"
    );

    for show_line_numbers in [true, false] {
        init_test(cx, |s| {
            s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
            s.defaults.tab_size = NonZeroU32::new(TAB_SIZE);
        });

        let actual_invisibles = collect_invisibles_from_new_editor(
            cx,
            EditorMode::full(),
            input_text,
            px(500.0),
            show_line_numbers,
        );

        assert_eq!(expected_invisibles, actual_invisibles);
    }
}

#[gpui::test]
fn test_multibyte_whitespace_uses_utf8_byte_offsets(cx: &mut TestAppContext) {
    init_test(cx, |s| {
        s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
    });

    // Regression test for #49186. NBSP (U+00A0) is rendered via the invisible
    // character `replacement` pipeline, which flushes the internal `line`
    // scratch buffer mid-line. Any whitespace invisible that follows must use
    // the absolute byte offset within the logical line (here: byte 4 for the
    // trailing ASCII space), not an offset relative to the post-flush buffer.
    let actual_invisibles =
        collect_invisibles_from_new_editor(cx, EditorMode::full(), "a\u{00A0}b ", px(500.0), false);

    assert_eq!(
        actual_invisibles,
        vec![Invisible::Whitespace {
            line_start_offset: 4,
            line_end_offset: 5,
        }]
    );
}

#[gpui::test]
fn test_replacement_chunks_are_clipped_to_max_line_len(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("", cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();
    let style = cx.update(|_, cx| editor.update(cx, |editor, cx| editor.style(cx).clone()));
    let editor_mode = EditorMode::full();
    let max_line_len = "\u{00a0}abcdef".len();

    window
        .update(cx, |_, window, cx| {
            let chunks = std::iter::once(HighlightedChunk {
                text: "\u{00a0}",
                style: None,
                is_tab: false,
                is_inlay: false,
                replacement: Some(ChunkReplacement::Str("\u{2007}".into())),
            })
            .chain(std::iter::once(HighlightedChunk {
                text: "abcdefghi",
                style: None,
                is_tab: false,
                is_inlay: false,
                replacement: None,
            }))
            .chain(
                std::iter::repeat_with(|| HighlightedChunk {
                    text: "\u{00a0}",
                    style: None,
                    is_tab: false,
                    is_inlay: false,
                    replacement: Some(ChunkReplacement::Str("\u{2007}".into())),
                })
                .take(8),
            );

            let layouts = LineWithInvisibles::from_chunks(
                chunks,
                &style,
                max_line_len,
                1,
                &editor_mode,
                px(500.),
                |_| false,
                &[],
                window,
                cx,
            );

            assert_eq!(layouts.len(), 1);
            assert_eq!(layouts[0].len, max_line_len);
            assert!(layouts[0].fragments.len() <= max_line_len);
        })
        .unwrap();
}

#[gpui::test]
fn test_invisibles_dont_appear_in_certain_editors(cx: &mut TestAppContext) {
    init_test(cx, |s| {
        s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
        s.defaults.tab_size = NonZeroU32::new(4);
    });

    for editor_mode_without_invisibles in [
        EditorMode::SingleLine,
        EditorMode::AutoHeight {
            min_lines: 1,
            max_lines: Some(100),
        },
    ] {
        for show_line_numbers in [true, false] {
            let invisibles = collect_invisibles_from_new_editor(
                cx,
                editor_mode_without_invisibles.clone(),
                "\t\t\t| | a b",
                px(500.0),
                show_line_numbers,
            );
            assert!(
                invisibles.is_empty(),
                "For editor mode {editor_mode_without_invisibles:?} no invisibles was expected but got {invisibles:?}"
            );
        }
    }
}

#[gpui::test]
fn test_wrapped_invisibles_drawing(cx: &mut TestAppContext) {
    let tab_size = 4;
    let input_text = "a\tbcd     ".repeat(9);
    let repeated_invisibles = [
        Invisible::Tab {
            line_start_offset: 1,
            line_end_offset: tab_size as usize,
        },
        Invisible::Whitespace {
            line_start_offset: tab_size as usize + 3,
            line_end_offset: tab_size as usize + 4,
        },
        Invisible::Whitespace {
            line_start_offset: tab_size as usize + 4,
            line_end_offset: tab_size as usize + 5,
        },
        Invisible::Whitespace {
            line_start_offset: tab_size as usize + 5,
            line_end_offset: tab_size as usize + 6,
        },
        Invisible::Whitespace {
            line_start_offset: tab_size as usize + 6,
            line_end_offset: tab_size as usize + 7,
        },
        Invisible::Whitespace {
            line_start_offset: tab_size as usize + 7,
            line_end_offset: tab_size as usize + 8,
        },
    ];
    let expected_invisibles = std::iter::once(repeated_invisibles)
        .cycle()
        .take(9)
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(
        expected_invisibles.len(),
        input_text
            .chars()
            .filter(|initial_char| initial_char.is_whitespace())
            .count(),
        "Hardcoded expected invisibles differ from the actual ones in '{input_text}'"
    );
    info!("Expected invisibles: {expected_invisibles:?}");

    init_test(cx, |_| {});

    // Put the same string with repeating whitespace pattern into editors of various size,
    // take deliberately small steps during resizing, to put all whitespace kinds near the wrap point.
    let resize_step = 10.0;
    let mut editor_width = 200.0;
    while editor_width <= 1000.0 {
        for show_line_numbers in [true, false] {
            update_test_language_settings(cx, &|s| {
                s.defaults.tab_size = NonZeroU32::new(tab_size);
                s.defaults.show_whitespaces = Some(ShowWhitespaceSetting::All);
                s.defaults.preferred_line_length = Some(editor_width as u32);
                s.defaults.soft_wrap = Some(language_settings::SoftWrap::Bounded);
            });

            let actual_invisibles = collect_invisibles_from_new_editor(
                cx,
                EditorMode::full(),
                &input_text,
                px(editor_width),
                show_line_numbers,
            );

            // Whatever the editor size is, ensure it has the same invisible kinds in the same order
            // (no good guarantees about the offsets: wrapping could trigger padding and its tests should check the offsets).
            let mut i = 0;
            for (actual_index, actual_invisible) in actual_invisibles.iter().enumerate() {
                i = actual_index;
                match expected_invisibles.get(i) {
                    Some(expected_invisible) => match (expected_invisible, actual_invisible) {
                        (Invisible::Whitespace { .. }, Invisible::Whitespace { .. })
                        | (Invisible::Tab { .. }, Invisible::Tab { .. }) => {}
                        _ => {
                            panic!(
                                "At index {i}, expected invisible {expected_invisible:?} does not match actual {actual_invisible:?} by kind. Actual invisibles: {actual_invisibles:?}"
                            )
                        }
                    },
                    None => {
                        panic!("Unexpected extra invisible {actual_invisible:?} at index {i}")
                    }
                }
            }
            let missing_expected_invisibles = &expected_invisibles[i + 1..];
            assert!(
                missing_expected_invisibles.is_empty(),
                "Missing expected invisibles after index {i}: {missing_expected_invisibles:?}"
            );

            editor_width += resize_step;
        }
    }
}

use super::*;

#[gpui::test]
fn test_autoindent_with_soft_tabs(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = "fn a() {}";
        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);

        buffer.edit([(8..8, "\n\n")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "fn a() {\n    \n}");

        buffer.edit(
            [(Point::new(1, 4)..Point::new(1, 4), "b()\n")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(buffer.text(), "fn a() {\n    b()\n    \n}");

        // Create a field expression on a new line, causing that line
        // to be indented.
        buffer.edit(
            [(Point::new(2, 4)..Point::new(2, 4), ".c")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(buffer.text(), "fn a() {\n    b()\n        .c\n}");

        // Remove the dot so that the line is no longer a field expression,
        // causing the line to be outdented.
        buffer.edit(
            [(Point::new(2, 8)..Point::new(2, 9), "")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(buffer.text(), "fn a() {\n    b()\n    c\n}");

        buffer
    });
}

#[gpui::test]
fn test_autoindent_with_hard_tabs(cx: &mut App) {
    init_settings(cx, |settings| {
        settings.defaults.hard_tabs = Some(true);
    });

    cx.new(|cx| {
        let text = "fn a() {}";
        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);

        buffer.edit([(8..8, "\n\n")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "fn a() {\n\t\n}");

        buffer.edit(
            [(Point::new(1, 1)..Point::new(1, 1), "b()\n")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(buffer.text(), "fn a() {\n\tb()\n\t\n}");

        // Create a field expression on a new line, causing that line
        // to be indented.
        buffer.edit(
            [(Point::new(2, 1)..Point::new(2, 1), ".c")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(buffer.text(), "fn a() {\n\tb()\n\t\t.c\n}");

        // Remove the dot so that the line is no longer a field expression,
        // causing the line to be outdented.
        buffer.edit(
            [(Point::new(2, 2)..Point::new(2, 3), "")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(buffer.text(), "fn a() {\n\tb()\n\tc\n}");

        buffer
    });
}

#[gpui::test]
fn test_autoindent_does_not_adjust_lines_with_unchanged_suggestion(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let mut buffer = Buffer::local(
            "
            fn a() {
            c;
            d;
            }
            "
            .unindent(),
            cx,
        )
        .with_language(rust_lang(), cx);

        // Lines 2 and 3 don't match the indentation suggestion. When editing these lines,
        // their indentation is not adjusted.
        buffer.edit_via_marked_text(
            &"
            fn a() {
            c«()»;
            d«()»;
            }
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {
            c();
            d();
            }
            "
            .unindent()
        );

        // When appending new content after these lines, the indentation is based on the
        // preceding lines' actual indentation.
        buffer.edit_via_marked_text(
            &"
            fn a() {
            c«
            .f
            .g()»;
            d«
            .f
            .g()»;
            }
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {
            c
                .f
                .g();
            d
                .f
                .g();
            }
            "
            .unindent()
        );

        // Insert a newline after the open brace. It is auto-indented
        buffer.edit_via_marked_text(
            &"
            fn a() {«
            »
            c
                .f
                .g();
            d
                .f
                .g();
            }
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {
                ˇ
            c
                .f
                .g();
            d
                .f
                .g();
            }
            "
            .unindent()
            .replace("ˇ", "")
        );

        // Manually outdent the line. It stays outdented.
        buffer.edit_via_marked_text(
            &"
            fn a() {
            «»
            c
                .f
                .g();
            d
                .f
                .g();
            }
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {

            c
                .f
                .g();
            d
                .f
                .g();
            }
            "
            .unindent()
        );

        buffer
    });

    cx.new(|cx| {
        eprintln!("second buffer: {:?}", cx.entity_id());

        let mut buffer = Buffer::local(
            "
            fn a() {
                b();
                |
            "
            .replace('|', "") // marker to preserve trailing whitespace
            .unindent(),
            cx,
        )
        .with_language(rust_lang(), cx);

        // Insert a closing brace. It is outdented.
        buffer.edit_via_marked_text(
            &"
            fn a() {
                b();
                «}»
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {
                b();
            }
            "
            .unindent()
        );

        // Manually edit the leading whitespace. The edit is preserved.
        buffer.edit_via_marked_text(
            &"
            fn a() {
                b();
            «    »}
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {
                b();
                }
            "
            .unindent()
        );
        buffer
    });

    eprintln!("DONE");
}

#[gpui::test]
fn test_autoindent_does_not_adjust_lines_within_newly_created_errors(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let mut buffer = Buffer::local(
            "
            fn a() {
                i
            }
            "
            .unindent(),
            cx,
        )
        .with_language(rust_lang(), cx);

        // Regression test: line does not get outdented due to syntax error
        buffer.edit_via_marked_text(
            &"
            fn a() {
                i«f let Some(x) = y»
            }
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {
                if let Some(x) = y
            }
            "
            .unindent()
        );

        buffer.edit_via_marked_text(
            &"
            fn a() {
                if let Some(x) = y« {»
            }
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a() {
                if let Some(x) = y {
            }
            "
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
fn test_autoindent_adjusts_lines_when_only_text_changes(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let mut buffer = Buffer::local(
            "
            fn a() {}
            "
            .unindent(),
            cx,
        )
        .with_language(rust_lang(), cx);

        buffer.edit_via_marked_text(
            &"
            fn a(«
            b») {}
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
            fn a(
                b) {}
            "
            .unindent()
        );

        // The indentation suggestion changed because `@end` node (a close paren)
        // is now at the beginning of the line.
        buffer.edit_via_marked_text(
            &"
            fn a(
                ˇ) {}
            "
            .unindent(),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
                fn a(
                ) {}
            "
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
fn test_autoindent_with_edit_at_end_of_buffer(cx: &mut App) {
    init_settings(cx, |_| {});

    cx.new(|cx| {
        let text = "a\nb";
        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);
        buffer.edit(
            [(0..1, "\n"), (2..3, "\n")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(buffer.text(), "\n\n\n");
        buffer
    });
}

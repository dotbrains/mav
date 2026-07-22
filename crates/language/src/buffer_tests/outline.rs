use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_outline(cx: &mut gpui::TestAppContext) {
    let text = r#"
        struct Person {
            name: String,
            age: usize,
        }

        mod module {
            enum LoginState {
                LoggedOut,
                LoggingOn,
                LoggedIn {
                    person: Person,
                    time: Instant,
                }
            }
        }

        impl Eq for Person {}

        impl Drop for Person {
            fn drop(&mut self) {
                println!("bye");
            }
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
    let outline = snapshot.outline(None);

    assert_eq!(
        outline
            .items
            .iter()
            .map(|item| (
                item.text.as_str(),
                item.depth,
                item.to_point(&snapshot).body_range(&snapshot)
                    .map(|range| minimize_space(&snapshot.text_for_range(range).collect::<String>()))
            ))
            .collect::<Vec<_>>(),
        &[
            ("struct Person", 0, Some("name: String, age: usize,".to_string())),
            ("name", 1, None),
            ("age", 1, None),
            (
                "mod module",
                0,
                Some(
                    "enum LoginState { LoggedOut, LoggingOn, LoggedIn { person: Person, time: Instant, } }".to_string()
                )
            ),
            (
                "enum LoginState",
                1,
                Some("LoggedOut, LoggingOn, LoggedIn { person: Person, time: Instant, }".to_string())
            ),
            ("LoggedOut", 2, None),
            ("LoggingOn", 2, None),
            ("LoggedIn", 2, Some("person: Person, time: Instant,".to_string())),
            ("person", 3, None),
            ("time", 3, None),
            ("impl Eq for Person", 0, Some("".to_string())),
            (
                "impl Drop for Person",
                0,
                Some("fn drop(&mut self) { println!(\"bye\"); }".to_string())
            ),
            ("fn drop", 1, Some("println!(\"bye\");".to_string())),
        ]
    );

    // Single-atom queries (no whitespace): all matched chars must land in the leaf,
    // so items whose ancestor path coincidentally contains the query chars don't
    // show up unless the leaf itself matches.
    assert_eq!(
        search(&outline, "oon", cx).await,
        &[
            ("mod module", vec![]),                     // parent context for LoggingOn
            ("enum LoginState", vec![]),                // parent context for LoggingOn
            ("LoggingOn", vec![1, 7, 8]),               // all three chars in leaf
            ("impl Eq for Person", vec![9, 16, 17]),    // o-o-n in "for Person"
            ("impl Drop for Person", vec![11, 18, 19]), // o-o-n in "for Person"
        ]
    );

    // Multi-atom queries: rows whose match lives entirely in an ancestor
    // are kept as context (empty positions, score zeroed) so descendants
    // of a matched container surface alongside it.
    assert_eq!(
        search(&outline, "dp p", cx).await,
        &[("impl Drop for Person", vec![5, 14]), ("fn drop", vec![]),]
    );
    assert_eq!(
        search(&outline, "dpn", cx).await,
        &[("impl Drop for Person", vec![5, 14, 19])]
    );
    assert_eq!(
        search(&outline, "impl ", cx).await,
        &[
            ("impl Eq for Person", vec![0, 1, 2, 3]),
            ("impl Drop for Person", vec![0, 1, 2, 3]),
            ("fn drop", vec![]),
        ]
    );

    fn minimize_space(text: &str) -> String {
        static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new("[\\n\\s]+").unwrap());
        WHITESPACE.replace_all(text, " ").trim().to_string()
    }

    async fn search<'a>(
        outline: &'a Outline<Anchor>,
        query: &'a str,
        cx: &'a gpui::TestAppContext,
    ) -> Vec<(&'a str, Vec<usize>)> {
        let entries = cx
            .update(|cx| outline.search(query, cx.background_executor().clone()))
            .await;
        entries
            .into_iter()
            .map(|entry| {
                let candidate_id = entry.candidate_id();
                let positions = entry.into_match().map(|m| m.positions).unwrap_or_default();
                (outline.items[candidate_id].text.as_str(), positions)
            })
            .collect::<Vec<_>>()
    }
}

#[gpui::test]
async fn test_outline_nodes_with_newlines(cx: &mut gpui::TestAppContext) {
    let text = r#"
        impl A for B<
            C
        > {
        };
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let outline = buffer.update(cx, |buffer, _| buffer.snapshot().outline(None));

    assert_eq!(
        outline
            .items
            .iter()
            .map(|item| (item.text.as_str(), item.depth))
            .collect::<Vec<_>>(),
        &[("impl A for B<", 0)]
    );
}

#[gpui::test]
async fn test_outline_with_extra_context(cx: &mut gpui::TestAppContext) {
    let language = javascript_lang()
        .with_outline_query(
            r#"
            (function_declaration
                "function" @context
                name: (_) @name
                parameters: (formal_parameters
                    "(" @context.extra
                    ")" @context.extra)) @item
            "#,
        )
        .unwrap();

    let text = r#"
        function a() {}
        function b(c) {}
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(Arc::new(language), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

    // extra context nodes are included in the outline.
    let outline = snapshot.outline(None);
    assert_eq!(
        outline
            .items
            .iter()
            .map(|item| (item.text.as_str(), item.depth))
            .collect::<Vec<_>>(),
        &[("function a()", 0), ("function b( )", 0),]
    );

    // extra context nodes do not appear in breadcrumbs.
    let symbols = snapshot.symbols_containing(3, None);
    assert_eq!(
        symbols
            .iter()
            .map(|item| (item.text.as_str(), item.depth))
            .collect::<Vec<_>>(),
        &[("function a", 0)]
    );
}

#[gpui::test]
async fn test_outline_selection_range_for_multiline_c_signature(cx: &mut gpui::TestAppContext) {
    let text = indoc! {"
        void
        evdev_post_scroll(struct evdev_device *device,
                  usec_t time,
                  enum libinput_pointer_axis_source source,
                  const struct normalized_coords *delta)
        {
            return;
        }
    "};

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(c_lang(), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
    let outline = snapshot.outline(None);

    let item = outline
        .items
        .iter()
        .find(|item| item.text.contains("evdev_post_scroll"))
        .unwrap()
        .to_point(&snapshot);

    assert_eq!(item.source_range_for_text.start, Point::new(0, 0));
    assert_eq!(item.selection_range.start, Point::new(1, 0));
    assert_eq!(item.text, "void evdev_post_scroll( )");
}

#[gpui::test]
fn test_outline_annotations(cx: &mut App) {
    // Add this new test case
    let text = r#"
        /// This is a doc comment
        /// that spans multiple lines
        fn annotated_function() {
            // This is not an annotation
        }

        // This is a single-line annotation
        fn another_function() {}

        fn unannotated_function() {}

        // This comment is not an annotation

        fn function_after_blank_line() {}
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let outline = buffer.update(cx, |buffer, _| buffer.snapshot().outline(None));

    assert_eq!(
        outline
            .items
            .into_iter()
            .map(|item| (
                item.text.to_string(),
                item.depth,
                item.annotation_range
                    .map(|range| { buffer.read(cx).text_for_range(range).collect::<String>() })
            ))
            .collect::<Vec<_>>(),
        &[
            (
                "fn annotated_function".to_string(),
                0,
                Some("/// This is a doc comment\n/// that spans multiple lines".to_string())
            ),
            (
                "fn another_function".to_string(),
                0,
                Some("// This is a single-line annotation".to_string())
            ),
            ("fn unannotated_function".to_string(), 0, None),
            ("fn function_after_blank_line".to_string(), 0, None),
        ]
    );
}

#[gpui::test]
async fn test_symbols_containing(cx: &mut gpui::TestAppContext) {
    let text = r#"
        impl Person {
            fn one() {
                1
            }

            fn two() {
                2
            }fn three() {
                3
            }
        }
    "#
    .unindent();

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

    // point is at the start of an item
    assert_eq!(
        symbols_containing(Point::new(1, 4), &snapshot),
        vec![
            (
                "impl Person".to_string(),
                Point::new(0, 0)..Point::new(10, 1)
            ),
            ("fn one".to_string(), Point::new(1, 4)..Point::new(3, 5))
        ]
    );

    // point is in the middle of an item
    assert_eq!(
        symbols_containing(Point::new(2, 8), &snapshot),
        vec![
            (
                "impl Person".to_string(),
                Point::new(0, 0)..Point::new(10, 1)
            ),
            ("fn one".to_string(), Point::new(1, 4)..Point::new(3, 5))
        ]
    );

    // point is at the end of an item
    assert_eq!(
        symbols_containing(Point::new(3, 5), &snapshot),
        vec![
            (
                "impl Person".to_string(),
                Point::new(0, 0)..Point::new(10, 1)
            ),
            ("fn one".to_string(), Point::new(1, 4)..Point::new(3, 5))
        ]
    );

    // point is in between two adjacent items
    assert_eq!(
        symbols_containing(Point::new(7, 5), &snapshot),
        vec![
            (
                "impl Person".to_string(),
                Point::new(0, 0)..Point::new(10, 1)
            ),
            ("fn two".to_string(), Point::new(5, 4)..Point::new(7, 5))
        ]
    );

    fn symbols_containing(
        position: Point,
        snapshot: &BufferSnapshot,
    ) -> Vec<(String, Range<Point>)> {
        snapshot
            .symbols_containing(position, None)
            .into_iter()
            .map(|item| {
                (
                    item.text.to_string(),
                    item.range.start.to_point(snapshot)..item.range.end.to_point(snapshot),
                )
            })
            .collect()
    }

    let (text, offsets) = marked_text_offsets(
        &"
        // ˇ😅 //
        fn test() {
        }
    "
        .unindent(),
    );
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

    // note, it would be nice to actually return the method test in this
    // case, but primarily asserting we don't crash because of the multibyte character.
    assert_eq!(snapshot.symbols_containing(offsets[0], None), vec![]);
}

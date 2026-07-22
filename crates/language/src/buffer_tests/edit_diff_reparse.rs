use super::*;

#[gpui::test]
fn test_edit_events(cx: &mut gpui::App) {
    let mut now = Instant::now();
    let buffer_1_events = Arc::new(Mutex::new(Vec::new()));
    let buffer_2_events = Arc::new(Mutex::new(Vec::new()));

    let buffer1 = cx.new(|cx| Buffer::local("abcdef", cx));
    let buffer2 = cx.new(|cx| {
        Buffer::remote(
            BufferId::from(cx.entity_id().as_non_zero_u64()),
            ReplicaId::new(1),
            Capability::ReadWrite,
            "abcdef",
        )
    });
    let buffer1_ops = Arc::new(Mutex::new(Vec::new()));
    buffer1.update(cx, {
        let buffer1_ops = buffer1_ops.clone();
        |buffer, cx| {
            let buffer_1_events = buffer_1_events.clone();
            cx.subscribe(&buffer1, move |_, _, event, _| match event.clone() {
                BufferEvent::Operation {
                    operation,
                    is_local: true,
                } => buffer1_ops.lock().push(operation),
                event => buffer_1_events.lock().push(event),
            })
            .detach();
            let buffer_2_events = buffer_2_events.clone();
            cx.subscribe(&buffer2, move |_, _, event, _| match event.clone() {
                BufferEvent::Operation {
                    is_local: false, ..
                } => {}
                event => buffer_2_events.lock().push(event),
            })
            .detach();

            // An edit emits an edited event, followed by a dirty changed event,
            // since the buffer was previously in a clean state.
            buffer.edit([(2..4, "XYZ")], None, cx);

            // An empty transaction does not emit any events.
            buffer.start_transaction();
            buffer.end_transaction(cx);

            // A transaction containing two edits emits one edited event.
            now += Duration::from_secs(1);
            buffer.start_transaction_at(now);
            buffer.edit([(5..5, "u")], None, cx);
            buffer.edit([(6..6, "w")], None, cx);
            buffer.end_transaction_at(now, cx);

            // Undoing a transaction emits one edited event.
            buffer.undo(cx);
        }
    });

    // Incorporating a set of remote ops emits a single edited event,
    // followed by a dirty changed event.
    buffer2.update(cx, |buffer, cx| {
        buffer.apply_ops(buffer1_ops.lock().drain(..), cx);
    });
    assert_eq!(
        mem::take(&mut *buffer_1_events.lock()),
        vec![
            BufferEvent::Edited {
                source: BufferEditSource::User
            },
            BufferEvent::DirtyChanged,
            BufferEvent::Edited {
                source: BufferEditSource::User
            },
            BufferEvent::Edited {
                source: BufferEditSource::User
            },
        ]
    );
    assert_eq!(
        mem::take(&mut *buffer_2_events.lock()),
        vec![
            BufferEvent::Edited {
                source: BufferEditSource::Remote
            },
            BufferEvent::DirtyChanged
        ]
    );

    buffer1.update(cx, |buffer, cx| {
        // Undoing the first transaction emits edited event, followed by a
        // dirty changed event, since the buffer is again in a clean state.
        buffer.undo(cx);
    });
    // Incorporating the remote ops again emits a single edited event,
    // followed by a dirty changed event.
    buffer2.update(cx, |buffer, cx| {
        buffer.apply_ops(buffer1_ops.lock().drain(..), cx);
    });
    assert_eq!(
        mem::take(&mut *buffer_1_events.lock()),
        vec![
            BufferEvent::Edited {
                source: BufferEditSource::User
            },
            BufferEvent::DirtyChanged,
        ]
    );
    assert_eq!(
        mem::take(&mut *buffer_2_events.lock()),
        vec![
            BufferEvent::Edited {
                source: BufferEditSource::Remote
            },
            BufferEvent::DirtyChanged
        ]
    );
}

#[gpui::test]
async fn test_apply_diff(cx: &mut TestAppContext) {
    let (text, offsets) = marked_text_offsets(
        "one two three\nfour fiˇve six\nseven eightˇ nine\nten eleven twelve\n",
    );
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let anchors = buffer.update(cx, |buffer, _| {
        offsets
            .iter()
            .map(|offset| buffer.anchor_before(offset))
            .collect::<Vec<_>>()
    });

    let (text, offsets) = marked_text_offsets(
        "one two three\n{\nfour FIVEˇ six\n}\nseven AND EIGHTˇ nine\nten eleven twelve\n",
    );

    let diff = buffer.update(cx, |b, cx| b.diff(text.clone(), cx)).await;
    buffer.update(cx, |buffer, cx| {
        buffer.apply_diff(diff, cx).unwrap();
        assert_eq!(buffer.text(), text);
        let actual_offsets = anchors
            .iter()
            .map(|anchor| anchor.to_offset(buffer))
            .collect::<Vec<_>>();
        assert_eq!(actual_offsets, offsets);
    });

    let (text, offsets) =
        marked_text_offsets("one two three\n{\nˇ}\nseven AND EIGHTEENˇ nine\nten eleven twelve\n");

    let diff = buffer.update(cx, |b, cx| b.diff(text.clone(), cx)).await;
    buffer.update(cx, |buffer, cx| {
        buffer.apply_diff(diff, cx).unwrap();
        assert_eq!(buffer.text(), text);
        let actual_offsets = anchors
            .iter()
            .map(|anchor| anchor.to_offset(buffer))
            .collect::<Vec<_>>();
        assert_eq!(actual_offsets, offsets);
    });
}

#[gpui::test(iterations = 10)]
async fn test_normalize_whitespace(cx: &mut gpui::TestAppContext) {
    let text = [
        "zero",     //
        "one  ",    // 2 trailing spaces
        "two",      //
        "three   ", // 3 trailing spaces
        "four",     //
        "five    ", // 4 trailing spaces
    ]
    .join("\n");

    let buffer = cx.new(|cx| Buffer::local(text, cx));

    // Spawn a task to format the buffer's whitespace.
    // Pause so that the formatting task starts running.
    let format = buffer.update(cx, |buffer, cx| buffer.remove_trailing_whitespace(cx));
    yield_now().await;

    // Edit the buffer while the normalization task is running.
    let version_before_edit = buffer.update(cx, |buffer, _| buffer.version());
    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (Point::new(0, 1)..Point::new(0, 1), "EE"),
                (Point::new(3, 5)..Point::new(3, 5), "EEE"),
            ],
            None,
            cx,
        );
    });

    let format_diff = format.await;
    buffer.update(cx, |buffer, cx| {
        let version_before_format = format_diff.base_version.clone();
        buffer.apply_diff(format_diff, cx);

        // The outcome depends on the order of concurrent tasks.
        //
        // If the edit occurred while searching for trailing whitespace ranges,
        // then the trailing whitespace region touched by the edit is left intact.
        if version_before_format == version_before_edit {
            assert_eq!(
                buffer.text(),
                [
                    "zEEero",      //
                    "one",         //
                    "two",         //
                    "threeEEE   ", //
                    "four",        //
                    "five",        //
                ]
                .join("\n")
            );
        }
        // Otherwise, all trailing whitespace is removed.
        else {
            assert_eq!(
                buffer.text(),
                [
                    "zEEero",   //
                    "one",      //
                    "two",      //
                    "threeEEE", //
                    "four",     //
                    "five",     //
                ]
                .join("\n")
            );
        }
    });
}

#[gpui::test]
async fn test_reparse(cx: &mut gpui::TestAppContext) {
    let text = "fn a() {}";
    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));

    // Wait for the initial text to parse
    cx.executor().run_until_parked();
    assert!(!buffer.update(cx, |buffer, _| buffer.is_parsing()));
    assert_eq!(
        get_tree_sexp(&buffer, cx),
        concat!(
            "(source_file (function_item name: (identifier) ",
            "parameters: (parameters) ",
            "body: (block)))"
        )
    );

    buffer.update(cx, |buffer, _| buffer.set_sync_parse_timeout(None));

    // Perform some edits (add parameter and variable reference)
    // Parsing doesn't begin until the transaction is complete
    buffer.update(cx, |buf, cx| {
        buf.start_transaction();

        let offset = buf.text().find(')').unwrap();
        buf.edit([(offset..offset, "b: C")], None, cx);
        assert!(!buf.is_parsing());

        let offset = buf.text().find('}').unwrap();
        buf.edit([(offset..offset, " d; ")], None, cx);
        assert!(!buf.is_parsing());

        buf.end_transaction(cx);
        assert_eq!(buf.text(), "fn a(b: C) { d; }");
        assert!(buf.is_parsing());
    });
    cx.executor().run_until_parked();
    assert!(!buffer.update(cx, |buffer, _| buffer.is_parsing()));
    assert_eq!(
        get_tree_sexp(&buffer, cx),
        concat!(
            "(source_file (function_item name: (identifier) ",
            "parameters: (parameters (parameter pattern: (identifier) type: (type_identifier))) ",
            "body: (block (expression_statement (identifier)))))"
        )
    );

    // Perform a series of edits without waiting for the current parse to complete:
    // * turn identifier into a field expression
    // * turn field expression into a method call
    // * add a turbofish to the method call
    buffer.update(cx, |buf, cx| {
        let offset = buf.text().find(';').unwrap();
        buf.edit([(offset..offset, ".e")], None, cx);
        assert_eq!(buf.text(), "fn a(b: C) { d.e; }");
        assert!(buf.is_parsing());
    });
    buffer.update(cx, |buf, cx| {
        let offset = buf.text().find(';').unwrap();
        buf.edit([(offset..offset, "(f)")], None, cx);
        assert_eq!(buf.text(), "fn a(b: C) { d.e(f); }");
        assert!(buf.is_parsing());
    });
    buffer.update(cx, |buf, cx| {
        let offset = buf.text().find("(f)").unwrap();
        buf.edit([(offset..offset, "::<G>")], None, cx);
        assert_eq!(buf.text(), "fn a(b: C) { d.e::<G>(f); }");
        assert!(buf.is_parsing());
    });
    cx.executor().run_until_parked();
    assert_eq!(
        get_tree_sexp(&buffer, cx),
        concat!(
            "(source_file (function_item name: (identifier) ",
            "parameters: (parameters (parameter pattern: (identifier) type: (type_identifier))) ",
            "body: (block (expression_statement (call_expression ",
            "function: (generic_function ",
            "function: (field_expression value: (identifier) field: (field_identifier)) ",
            "type_arguments: (type_arguments (type_identifier))) ",
            "arguments: (arguments (identifier)))))))",
        )
    );

    buffer.update(cx, |buf, cx| {
        buf.undo(cx);
        buf.undo(cx);
        buf.undo(cx);
        buf.undo(cx);
        assert_eq!(buf.text(), "fn a() {}");
        assert!(buf.is_parsing());
    });

    cx.executor().run_until_parked();
    assert_eq!(
        get_tree_sexp(&buffer, cx),
        concat!(
            "(source_file (function_item name: (identifier) ",
            "parameters: (parameters) ",
            "body: (block)))"
        )
    );

    buffer.update(cx, |buf, cx| {
        buf.redo(cx);
        buf.redo(cx);
        buf.redo(cx);
        buf.redo(cx);
        assert_eq!(buf.text(), "fn a(b: C) { d.e::<G>(f); }");
        assert!(buf.is_parsing());
    });
    cx.executor().run_until_parked();
    assert_eq!(
        get_tree_sexp(&buffer, cx),
        concat!(
            "(source_file (function_item name: (identifier) ",
            "parameters: (parameters (parameter pattern: (identifier) type: (type_identifier))) ",
            "body: (block (expression_statement (call_expression ",
            "function: (generic_function ",
            "function: (field_expression value: (identifier) field: (field_identifier)) ",
            "type_arguments: (type_arguments (type_identifier))) ",
            "arguments: (arguments (identifier)))))))",
        )
    );
}

#[gpui::test]
async fn test_resetting_language(cx: &mut gpui::TestAppContext) {
    let buffer = cx.new(|cx| {
        let mut buffer = Buffer::local("{}", cx).with_language(rust_lang(), cx);
        buffer.set_sync_parse_timeout(None);
        buffer
    });

    // Wait for the initial text to parse
    cx.executor().run_until_parked();
    assert_eq!(
        get_tree_sexp(&buffer, cx),
        "(source_file (expression_statement (block)))"
    );

    buffer.update(cx, |buffer, cx| {
        buffer.set_language(Some(Arc::new(json_lang())), cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(get_tree_sexp(&buffer, cx), "(document (object))");
}

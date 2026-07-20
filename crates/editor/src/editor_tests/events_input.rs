use super::*;

#[gpui::test]
fn test_edit_events(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer = cx.new(|cx| {
        let mut buffer = language::Buffer::local("123456", cx);
        buffer.set_group_interval(Duration::from_secs(1));
        buffer
    });

    let events = Rc::new(RefCell::new(Vec::new()));
    let editor1 = cx.add_window({
        let events = events.clone();
        |window, cx| {
            let entity = cx.entity();
            cx.subscribe_in(
                &entity,
                window,
                move |_, _, event: &EditorEvent, _, _| match event {
                    EditorEvent::Edited { .. } => events.borrow_mut().push(("editor1", "edited")),
                    EditorEvent::BufferEdited => {
                        events.borrow_mut().push(("editor1", "buffer edited"))
                    }
                    _ => {}
                },
            )
            .detach();
            Editor::for_buffer(buffer.clone(), None, window, cx)
        }
    });

    let editor2 = cx.add_window({
        let events = events.clone();
        |window, cx| {
            cx.subscribe_in(
                &cx.entity(),
                window,
                move |_, _, event: &EditorEvent, _, _| match event {
                    EditorEvent::Edited { .. } => events.borrow_mut().push(("editor2", "edited")),
                    EditorEvent::BufferEdited => {
                        events.borrow_mut().push(("editor2", "buffer edited"))
                    }
                    _ => {}
                },
            )
            .detach();
            Editor::for_buffer(buffer.clone(), None, window, cx)
        }
    });

    assert_eq!(mem::take(&mut *events.borrow_mut()), []);

    // Mutating editor 1 will emit an `Edited` event only for that editor.
    _ = editor1.update(cx, |editor, window, cx| editor.insert("X", window, cx));
    assert_eq!(
        mem::take(&mut *events.borrow_mut()),
        [
            ("editor1", "edited"),
            ("editor1", "buffer edited"),
            ("editor2", "buffer edited"),
        ]
    );

    // Mutating editor 2 will emit an `Edited` event only for that editor.
    _ = editor2.update(cx, |editor, window, cx| editor.delete(&Delete, window, cx));
    assert_eq!(
        mem::take(&mut *events.borrow_mut()),
        [
            ("editor2", "edited"),
            ("editor1", "buffer edited"),
            ("editor2", "buffer edited"),
        ]
    );

    // Undoing on editor 1 will emit an `Edited` event only for that editor.
    _ = editor1.update(cx, |editor, window, cx| editor.undo(&Undo, window, cx));
    assert_eq!(
        mem::take(&mut *events.borrow_mut()),
        [
            ("editor1", "edited"),
            ("editor1", "buffer edited"),
            ("editor2", "buffer edited"),
        ]
    );

    // Redoing on editor 1 will emit an `Edited` event only for that editor.
    _ = editor1.update(cx, |editor, window, cx| editor.redo(&Redo, window, cx));
    assert_eq!(
        mem::take(&mut *events.borrow_mut()),
        [
            ("editor1", "edited"),
            ("editor1", "buffer edited"),
            ("editor2", "buffer edited"),
        ]
    );

    // Undoing on editor 2 will emit an `Edited` event only for that editor.
    _ = editor2.update(cx, |editor, window, cx| editor.undo(&Undo, window, cx));
    assert_eq!(
        mem::take(&mut *events.borrow_mut()),
        [
            ("editor2", "edited"),
            ("editor1", "buffer edited"),
            ("editor2", "buffer edited"),
        ]
    );

    // Redoing on editor 2 will emit an `Edited` event only for that editor.
    _ = editor2.update(cx, |editor, window, cx| editor.redo(&Redo, window, cx));
    assert_eq!(
        mem::take(&mut *events.borrow_mut()),
        [
            ("editor2", "edited"),
            ("editor1", "buffer edited"),
            ("editor2", "buffer edited"),
        ]
    );

    // No event is emitted when the mutation is a no-op.
    _ = editor2.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(0)])
        });

        editor.backspace(&Backspace, window, cx);
    });
    assert_eq!(mem::take(&mut *events.borrow_mut()), []);
}

#[gpui::test]
fn test_undo_redo_with_selection_restoration(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut now = Instant::now();
    let group_interval = Duration::from_millis(1);
    let buffer = cx.new(|cx| {
        let mut buf = language::Buffer::local("123456", cx);
        buf.set_group_interval(group_interval);
        buf
    });
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let editor = cx.add_window(|window, cx| build_editor(buffer.clone(), window, cx));

    _ = editor.update(cx, |editor, window, cx| {
        editor.start_transaction_at(now, window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(4)])
        });

        editor.insert("cd", window, cx);
        editor.end_transaction_at(now, cx);
        assert_eq!(editor.text(cx), "12cd56");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(4)..MultiBufferOffset(4)]
        );

        editor.start_transaction_at(now, window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(4)..MultiBufferOffset(5)])
        });
        editor.insert("e", window, cx);
        editor.end_transaction_at(now, cx);
        assert_eq!(editor.text(cx), "12cde6");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(5)..MultiBufferOffset(5)]
        );

        now += group_interval + Duration::from_millis(1);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(2)])
        });

        // Simulate an edit in another editor
        buffer.update(cx, |buffer, cx| {
            buffer.start_transaction_at(now, cx);
            buffer.edit(
                [(MultiBufferOffset(0)..MultiBufferOffset(1), "a")],
                None,
                cx,
            );
            buffer.edit(
                [(MultiBufferOffset(1)..MultiBufferOffset(1), "b")],
                None,
                cx,
            );
            buffer.end_transaction_at(now, cx);
        });

        assert_eq!(editor.text(cx), "ab2cde6");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(3)..MultiBufferOffset(3)]
        );

        // Last transaction happened past the group interval in a different editor.
        // Undo it individually and don't restore selections.
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "12cde6");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(2)..MultiBufferOffset(2)]
        );

        // First two transactions happened within the group interval in this editor.
        // Undo them together and restore selections.
        editor.undo(&Undo, window, cx);
        editor.undo(&Undo, window, cx); // Undo stack is empty here, so this is a no-op.
        assert_eq!(editor.text(cx), "123456");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(0)..MultiBufferOffset(0)]
        );

        // Redo the first two transactions together.
        editor.redo(&Redo, window, cx);
        assert_eq!(editor.text(cx), "12cde6");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(5)..MultiBufferOffset(5)]
        );

        // Redo the last transaction on its own.
        editor.redo(&Redo, window, cx);
        assert_eq!(editor.text(cx), "ab2cde6");
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(6)..MultiBufferOffset(6)]
        );

        // Test empty transactions.
        editor.start_transaction_at(now, window, cx);
        editor.end_transaction_at(now, cx);
        editor.undo(&Undo, window, cx);
        assert_eq!(editor.text(cx), "12cde6");
    });
}

#[gpui::test]
fn test_accessibility_keyboard_word_completion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Simulates the macOS Accessibility Keyboard word completion panel, which calls
    // insertText:replacementRange: to commit a completion. macOS sends two calls per
    // completion: one with a non-empty range replacing the typed prefix, and one with
    // an empty replacement range (cursor..cursor) to append a trailing space.

    cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("ab", cx);
        let mut editor = build_editor(buffer, window, cx);

        // Cursor is after the 2-char prefix "ab" at offset 2.
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(2)])
        });

        // macOS completes "about" by replacing the prefix via range 0..2.
        editor.replace_text_in_range(Some(0..2), "about", window, cx);
        assert_eq!(editor.text(cx), "about");

        // macOS sends a trailing space as an empty replacement range (cursor..cursor).
        // Must insert at the cursor position, not call backspace first (which would
        // delete the preceding character).
        editor.replace_text_in_range(Some(5..5), " ", window, cx);
        assert_eq!(editor.text(cx), "about ");

        editor
    });

    // Multi-cursor: the replacement must fan out to all cursors, and the trailing
    // space must land at each cursor's actual current position. After the first
    // completion, macOS's reported cursor offset is stale (it doesn't account for
    // the offset shift caused by the other cursor's insertion), so the empty
    // replacement range must be ignored and the space inserted at each real cursor.
    cx.add_window(|window, cx| {
        // Two cursors, each after a 2-char prefix "ab" at the end of each line:
        //   "ab\nab" — cursors at offsets 2 and 5.
        let buffer = MultiBuffer::build_simple("ab\nab", cx);
        let mut editor = build_editor(buffer, window, cx);

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffset(2)..MultiBufferOffset(2),
                MultiBufferOffset(5)..MultiBufferOffset(5),
            ])
        });

        // macOS reports the newest cursor (offset 5) and sends range 3..5 to
        // replace its 2-char prefix. selection_replacement_ranges applies the same
        // delta to fan out to both cursors: 0..2 and 3..5.
        editor.replace_text_in_range(Some(3..5), "about", window, cx);
        assert_eq!(editor.text(cx), "about\nabout");

        // Trailing space via empty range. macOS thinks the cursor is at offset 10
        // (5 - 2 + 7 = 10), but the actual cursors are at 5 and 11. The stale
        // offset must be ignored and the space inserted at each real cursor position.
        editor.replace_text_in_range(Some(10..10), " ", window, cx);
        assert_eq!(editor.text(cx), "about \nabout ");

        editor
    });
}

#[gpui::test]
fn test_ime_composition(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer = cx.new(|cx| {
        let mut buffer = language::Buffer::local("abcde", cx);
        // Ensure automatic grouping doesn't occur.
        buffer.set_group_interval(Duration::ZERO);
        buffer
    });

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    cx.add_window(|window, cx| {
        let mut editor = build_editor(buffer.clone(), window, cx);

        // Start a new IME composition.
        editor.replace_and_mark_text_in_range(Some(0..1), "à", None, window, cx);
        editor.replace_and_mark_text_in_range(Some(0..1), "á", None, window, cx);
        editor.replace_and_mark_text_in_range(Some(0..1), "ä", None, window, cx);
        assert_eq!(editor.text(cx), "äbcde");
        assert_eq!(
            editor.marked_text_ranges(cx),
            Some(vec![
                MultiBufferOffsetUtf16(OffsetUtf16(0))..MultiBufferOffsetUtf16(OffsetUtf16(1))
            ])
        );

        // Finalize IME composition.
        editor.replace_text_in_range(None, "ā", window, cx);
        assert_eq!(editor.text(cx), "ābcde");
        assert_eq!(editor.marked_text_ranges(cx), None);

        // IME composition edits are grouped and are undone/redone at once.
        editor.undo(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "abcde");
        assert_eq!(editor.marked_text_ranges(cx), None);
        editor.redo(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "ābcde");
        assert_eq!(editor.marked_text_ranges(cx), None);

        // Start a new IME composition.
        editor.replace_and_mark_text_in_range(Some(0..1), "à", None, window, cx);
        assert_eq!(
            editor.marked_text_ranges(cx),
            Some(vec![
                MultiBufferOffsetUtf16(OffsetUtf16(0))..MultiBufferOffsetUtf16(OffsetUtf16(1))
            ])
        );

        // Undoing during an IME composition cancels it.
        editor.undo(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "ābcde");
        assert_eq!(editor.marked_text_ranges(cx), None);

        // Start a new IME composition with an invalid marked range, ensuring it gets clipped.
        editor.replace_and_mark_text_in_range(Some(4..999), "è", None, window, cx);
        assert_eq!(editor.text(cx), "ābcdè");
        assert_eq!(
            editor.marked_text_ranges(cx),
            Some(vec![
                MultiBufferOffsetUtf16(OffsetUtf16(4))..MultiBufferOffsetUtf16(OffsetUtf16(5))
            ])
        );

        // Finalize IME composition with an invalid replacement range, ensuring it gets clipped.
        editor.replace_text_in_range(Some(4..999), "ę", window, cx);
        assert_eq!(editor.text(cx), "ābcdę");
        assert_eq!(editor.marked_text_ranges(cx), None);

        // Start a new IME composition with multiple cursors.
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffsetUtf16(OffsetUtf16(1))..MultiBufferOffsetUtf16(OffsetUtf16(1)),
                MultiBufferOffsetUtf16(OffsetUtf16(3))..MultiBufferOffsetUtf16(OffsetUtf16(3)),
                MultiBufferOffsetUtf16(OffsetUtf16(5))..MultiBufferOffsetUtf16(OffsetUtf16(5)),
            ])
        });
        editor.replace_and_mark_text_in_range(Some(4..5), "XYZ", None, window, cx);
        assert_eq!(editor.text(cx), "XYZbXYZdXYZ");
        assert_eq!(
            editor.marked_text_ranges(cx),
            Some(vec![
                MultiBufferOffsetUtf16(OffsetUtf16(0))..MultiBufferOffsetUtf16(OffsetUtf16(3)),
                MultiBufferOffsetUtf16(OffsetUtf16(4))..MultiBufferOffsetUtf16(OffsetUtf16(7)),
                MultiBufferOffsetUtf16(OffsetUtf16(8))..MultiBufferOffsetUtf16(OffsetUtf16(11))
            ])
        );

        // Ensure the newly-marked range gets treated as relative to the previously-marked ranges.
        editor.replace_and_mark_text_in_range(Some(1..2), "1", None, window, cx);
        assert_eq!(editor.text(cx), "X1ZbX1ZdX1Z");
        assert_eq!(
            editor.marked_text_ranges(cx),
            Some(vec![
                MultiBufferOffsetUtf16(OffsetUtf16(1))..MultiBufferOffsetUtf16(OffsetUtf16(2)),
                MultiBufferOffsetUtf16(OffsetUtf16(5))..MultiBufferOffsetUtf16(OffsetUtf16(6)),
                MultiBufferOffsetUtf16(OffsetUtf16(9))..MultiBufferOffsetUtf16(OffsetUtf16(10))
            ])
        );

        // Finalize IME composition with multiple cursors.
        editor.replace_text_in_range(Some(9..10), "2", window, cx);
        assert_eq!(editor.text(cx), "X2ZbX2ZdX2Z");
        assert_eq!(editor.marked_text_ranges(cx), None);

        editor
    });
}

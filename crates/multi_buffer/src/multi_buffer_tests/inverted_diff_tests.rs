use super::*;

#[gpui::test]
async fn test_singleton_with_inverted_diff(cx: &mut TestAppContext) {
    let text = indoc!(
        "
        ZERO
        one
        TWO
        three
        six
        "
    );
    let base_text = indoc!(
        "
        one
        two
        three
        four
        five
        six
        "
    );

    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();

    let base_text_buffer = diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::singleton(base_text_buffer.clone(), cx);
        multibuffer.set_all_diff_hunks_expanded(cx);
        multibuffer.add_inverted_diff(diff.clone(), buffer.clone(), cx);
        multibuffer
    });

    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    assert_eq!(snapshot.text(), base_text);
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
              one
            - two
              three
            - four
            - five
              six
            "
        ),
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit_via_marked_text(
            indoc!(
                "
                ZERO
                one
                «<inserted>»W«O
                T»hree
                six
                "
            ),
            None,
            cx,
        );
    });
    cx.run_until_parked();
    let base_text_snapshot = diff.read_with(cx, |diff, cx| diff.base_text(cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.text_snapshot());
    let update = diff
        .update(cx, |diff, cx| {
            diff.update_diff(
                buffer_snapshot,
                &base_text_snapshot,
                Some(Arc::from(base_text)),
                cx,
            )
        })
        .await;
    diff.update(cx, |diff, cx| diff.set_snapshot(update, cx));
    cx.run_until_parked();

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
              one
            - two
            - three
            - four
            - five
              six
            "
        },
    );

    buffer.update(cx, |buffer, cx| {
        buffer.set_text("ZERO\nONE\nTWO\n", cx);
    });
    cx.run_until_parked();
    let base_text_snapshot = diff.read_with(cx, |diff, cx| diff.base_text(cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.text_snapshot());
    let update = diff
        .update(cx, |diff, cx| {
            diff.update_diff(
                buffer_snapshot,
                &base_text_snapshot,
                Some(Arc::from(base_text)),
                cx,
            )
        })
        .await;
    diff.update(cx, |diff, cx| diff.set_snapshot(update, cx));
    cx.run_until_parked();

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
            - one
            - two
            - three
            - four
            - five
            - six
            "
        },
    );

    diff.update(cx, |diff, cx| {
        diff.set_base_text(
            Some("new base\n".into()),
            buffer.read(cx).text_snapshot(),
            cx,
        )
    })
    .await;
    cx.run_until_parked();

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {"
            - new base
        "},
    );
}

#[gpui::test]
async fn test_inverted_diff_base_text_change(cx: &mut TestAppContext) {
    let base_text = "aaa\nbbb\nccc\n";
    let text = "ddd\n";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();

    let base_text_buffer = diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::singleton(base_text_buffer.clone(), cx);
        multibuffer.set_all_diff_hunks_expanded(cx);
        multibuffer.add_inverted_diff(diff.clone(), buffer.clone(), cx);
        multibuffer
    });

    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    assert_eq!(snapshot.text(), base_text);
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
            - aaa
            - bbb
            - ccc
            "
        ),
    );

    diff.update(cx, |diff, cx| {
        diff.set_base_text(Some("ddd\n".into()), buffer.read(cx).text_snapshot(), cx)
    })
    .await;

    let _hunks: Vec<_> = multibuffer
        .read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx))
        .diff_hunks()
        .collect();
}

#[gpui::test]
async fn test_inverted_diff_secondary_version_mismatch(cx: &mut TestAppContext) {
    let base_text = "one\ntwo\nthree\nfour\nfive\n";
    let index_text = "one\nTWO\nthree\nfour\nfive\n";
    let buffer_text = "one\nTWO\nthree\nFOUR\nfive\n";

    let buffer = cx.new(|cx| Buffer::local(buffer_text, cx));

    let unstaged_diff = cx
        .new(|cx| BufferDiff::new_with_base_text(index_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();

    let uncommitted_diff = cx.new(|cx| {
        let mut diff =
            BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx);
        diff.set_secondary_diff(unstaged_diff.clone());
        diff
    });
    cx.run_until_parked();

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "ZERO\n")], None, cx);
    });

    let base_text_snapshot = unstaged_diff.read_with(cx, |diff, cx| diff.base_text(cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.text_snapshot());
    let update = unstaged_diff
        .update(cx, |diff, cx| {
            diff.update_diff(
                buffer_snapshot,
                &base_text_snapshot,
                Some(Arc::from(index_text)),
                cx,
            )
        })
        .await;
    unstaged_diff.update(cx, |diff, cx| diff.set_snapshot(update, cx));

    let base_text_buffer =
        uncommitted_diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::singleton(base_text_buffer.clone(), cx);
        multibuffer.set_all_diff_hunks_expanded(cx);
        multibuffer.add_inverted_diff(uncommitted_diff.clone(), buffer.clone(), cx);
        multibuffer
    });

    let _hunks: Vec<_> = multibuffer
        .read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx))
        .diff_hunks()
        .collect();
}

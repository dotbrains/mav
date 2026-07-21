use super::*;

#[gpui::test]
fn test_cannot_seek_backward_after_excerpt_replacement(cx: &mut TestAppContext) {
    let buffer_b_text: String = (0..50).map(|i| format!("line_b {i}\n")).collect();
    let buffer_b = cx.new(|cx| Buffer::local(buffer_b_text, cx));

    let buffer_c_text: String = (0..10).map(|i| format!("line_c {i}\n")).collect();
    let buffer_c = cx.new(|cx| Buffer::local(buffer_c_text, cx));

    let buffer_d_text: String = (0..10).map(|i| format!("line_d {i}\n")).collect();
    let buffer_d = cx.new(|cx| Buffer::local(buffer_d_text, cx));

    let path_b = PathKey::with_sort_prefix(0, rel_path("bbb.rs").into_arc());
    let path_c = PathKey::with_sort_prefix(0, rel_path("ddd.rs").into_arc());
    let path_d = PathKey::with_sort_prefix(0, rel_path("ccc.rs").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path_b.clone(),
            buffer_b.clone(),
            vec![
                Point::row_range(0..3),
                Point::row_range(15..18),
                Point::row_range(30..33),
            ],
            0,
            cx,
        );
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path_c.clone(),
            buffer_c.clone(),
            vec![Point::row_range(0..3)],
            0,
            cx,
        );
    });

    let (anchor_in_e_b2, anchor_in_e_b3) = multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        let excerpt_infos = snapshot.excerpts().collect::<Vec<_>>();
        assert_eq!(excerpt_infos.len(), 4, "expected 4 excerpts (3×B + 1×C)");

        let e_b2_info = excerpt_infos[1].clone();
        let e_b3_info = excerpt_infos[2].clone();

        let anchor_b2 = snapshot.anchor_in_excerpt(e_b2_info.context.start).unwrap();
        let anchor_b3 = snapshot.anchor_in_excerpt(e_b3_info.context.start).unwrap();
        (anchor_b2, anchor_b3)
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path_b.clone(),
            buffer_b.clone(),
            vec![Point::row_range(0..3), Point::row_range(28..36)],
            0,
            cx,
        );
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path_d.clone(),
            buffer_d.clone(),
            vec![Point::row_range(0..3)],
            0,
            cx,
        );
    });

    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        snapshot.summaries_for_anchors::<Point, _>(&[anchor_in_e_b2, anchor_in_e_b3]);
    });
}

#[gpui::test]
fn test_resolving_max_anchor_for_buffer(cx: &mut TestAppContext) {
    let dock_base_text = indoc! {"
        0
        1
        2
        3
        4
        5
        6
        7
        8
        9
        10
        11
        12
    "};

    let dock_text = indoc! {"
        0
        4
        5
        6
        10
        11
        12
    "};

    let dock_buffer = cx.new(|cx| Buffer::local(dock_text, cx));
    let diff = cx.new(|cx| {
        BufferDiff::new_with_base_text(dock_base_text, &dock_buffer.read(cx).snapshot(), cx)
    });

    let workspace_text = "second buffer\n";
    let workspace_buffer = cx.new(|cx| Buffer::local(workspace_text, cx));

    let dock_path = PathKey::with_sort_prefix(0, rel_path("").into_arc());
    let workspace_path = PathKey::with_sort_prefix(1, rel_path("").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpt_ranges_for_path(
            dock_path,
            dock_buffer.clone(),
            &dock_buffer.read(cx).snapshot(),
            vec![
                ExcerptRange::new(Point::zero()..Point::new(1, 1)),
                ExcerptRange::new(Point::new(3, 0)..Point::new(4, 2)),
            ],
            cx,
        );
        multibuffer.set_excerpt_ranges_for_path(
            workspace_path,
            workspace_buffer.clone(),
            &workspace_buffer.read(cx).snapshot(),
            vec![ExcerptRange::new(
                Point::zero()..workspace_buffer.read(cx).max_point(),
            )],
            cx,
        );
        multibuffer.add_diff(diff, cx);
        multibuffer.set_all_diff_hunks_expanded(cx);
    });

    let snapshot = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    let diff = format_diff(
        &snapshot.text(),
        &snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>(),
        &Default::default(),
        None,
    );
    assert_eq!(
        diff,
        indoc! {"
            0
          - 1
          - 2
          - 3
            4 [↓]
            6 [↑]
          - 7
          - 8
          - 9
            10 [↓]
            second buffer
        "}
    );

    multibuffer.update(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        let point = snapshot
            .anchor_in_buffer(text::Anchor::max_for_buffer(
                dock_buffer.read(cx).remote_id(),
            ))
            .unwrap()
            .to_point(&snapshot);
        assert_eq!(point, Point::new(10, 0));
    })
}

#[gpui::test]
fn test_is_valid_anchor_past_last_excerpt_for_buffer(cx: &mut TestAppContext) {
    let buffer_a = cx.new(|cx| Buffer::local("aaa\nbbb\nccc\n", cx));
    buffer_a.update(cx, |buffer, cx| {
        let len = buffer.len();
        buffer.edit([(len..len, "ddd\neee\n")], None, cx);
    });
    let buffer_b = cx.new(|cx| Buffer::local("xxx\n", cx));
    for line in ["yyy\n", "zzz\n", "www\n", "vvv\n"] {
        buffer_b.update(cx, |buffer, cx| {
            let len = buffer.len();
            buffer.edit([(len..len, line)], None, cx);
        });
    }

    let path_a = PathKey::with_sort_prefix(0, rel_path("aaa.rs").into_arc());
    let path_b = PathKey::with_sort_prefix(1, rel_path("bbb.rs").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path_a.clone(),
            buffer_a.clone(),
            vec![Point::new(1, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            path_b.clone(),
            buffer_b.clone(),
            vec![Point::new(1, 0)..Point::new(3, 3)],
            0,
            cx,
        );
    });

    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);

        let buffer_a_snapshot = buffer_a.read(cx).snapshot();
        let anchor_past_excerpt = buffer_a_snapshot.anchor_after(Point::new(4, 0));
        let mb_anchor = snapshot.anchor_in_buffer(anchor_past_excerpt).unwrap();

        assert!(
            !mb_anchor.is_valid(&snapshot),
            "anchor past the last excerpt for its buffer should not be valid"
        );
    });
}

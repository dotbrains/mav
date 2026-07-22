use super::*;

#[gpui::test]
fn test_serialization(cx: &mut gpui::App) {
    let mut now = Instant::now();

    let buffer1 = cx.new(|cx| {
        let mut buffer = Buffer::local("abc", cx);
        buffer.edit([(3..3, "D")], None, cx);

        now += Duration::from_secs(1);
        buffer.start_transaction_at(now);
        buffer.edit([(4..4, "E")], None, cx);
        buffer.end_transaction_at(now, cx);
        assert_eq!(buffer.text(), "abcDE");

        buffer.undo(cx);
        assert_eq!(buffer.text(), "abcD");

        buffer.edit([(4..4, "F")], None, cx);
        assert_eq!(buffer.text(), "abcDF");
        buffer
    });
    assert_eq!(buffer1.read(cx).text(), "abcDF");

    let state = buffer1.read(cx).to_proto(cx);
    let ops = cx
        .foreground_executor()
        .block_on(buffer1.read(cx).serialize_ops(None, cx));
    let buffer2 = cx.new(|cx| {
        let mut buffer =
            Buffer::from_proto(ReplicaId::new(1), Capability::ReadWrite, state, None).unwrap();
        buffer.apply_ops(
            ops.into_iter()
                .map(|op| proto::deserialize_operation(op).unwrap()),
            cx,
        );
        buffer
    });
    assert_eq!(buffer2.read(cx).text(), "abcDF");
}

#[gpui::test]
fn test_branch_and_merge(cx: &mut TestAppContext) {
    cx.update(|cx| init_settings(cx, |_| {}));

    let base = cx.new(|cx| Buffer::local("one\ntwo\nthree\n", cx));

    // Create a remote replica of the base buffer.
    let base_replica = cx.new(|cx| {
        Buffer::from_proto(
            ReplicaId::new(1),
            Capability::ReadWrite,
            base.read(cx).to_proto(cx),
            None,
        )
        .unwrap()
    });
    base.update(cx, |_buffer, cx| {
        cx.subscribe(&base_replica, |this, _, event, cx| {
            if let BufferEvent::Operation {
                operation,
                is_local: true,
            } = event
            {
                this.apply_ops([operation.clone()], cx);
            }
        })
        .detach();
    });

    // Create a branch, which initially has the same state as the base buffer.
    let branch = base.update(cx, |buffer, cx| buffer.branch(cx));
    branch.read_with(cx, |buffer, _| {
        assert_eq!(buffer.text(), "one\ntwo\nthree\n");
    });

    // Edits to the branch are not applied to the base.
    branch.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (Point::new(1, 0)..Point::new(1, 0), "1.5\n"),
                (Point::new(2, 0)..Point::new(2, 5), "THREE"),
            ],
            None,
            cx,
        )
    });
    branch.read_with(cx, |buffer, cx| {
        assert_eq!(base.read(cx).text(), "one\ntwo\nthree\n");
        assert_eq!(buffer.text(), "one\n1.5\ntwo\nTHREE\n");
    });

    // Convert from branch buffer ranges to the corresponding ranges in the
    // base buffer.
    branch.read_with(cx, |buffer, cx| {
        assert_eq!(
            buffer.range_to_version(4..7, &base.read(cx).version()),
            4..4
        );
        assert_eq!(
            buffer.range_to_version(2..9, &base.read(cx).version()),
            2..5
        );
    });

    // Edits to the base are applied to the branch.
    base.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(0, 0)..Point::new(0, 0), "ZERO\n")], None, cx)
    });
    branch.read_with(cx, |buffer, cx| {
        assert_eq!(base.read(cx).text(), "ZERO\none\ntwo\nthree\n");
        assert_eq!(buffer.text(), "ZERO\none\n1.5\ntwo\nTHREE\n");
    });

    // Edits to any replica of the base are applied to the branch.
    base_replica.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(2, 0)..Point::new(2, 0), "2.5\n")], None, cx)
    });
    branch.read_with(cx, |buffer, cx| {
        assert_eq!(base.read(cx).text(), "ZERO\none\ntwo\n2.5\nthree\n");
        assert_eq!(buffer.text(), "ZERO\none\n1.5\ntwo\n2.5\nTHREE\n");
    });

    // Merging the branch applies all of its changes to the base.
    branch.update(cx, |buffer, cx| {
        buffer.merge_into_base(Vec::new(), cx);
    });

    branch.update(cx, |buffer, cx| {
        assert_eq!(base.read(cx).text(), "ZERO\none\n1.5\ntwo\n2.5\nTHREE\n");
        assert_eq!(buffer.text(), "ZERO\none\n1.5\ntwo\n2.5\nTHREE\n");
    });
}

#[gpui::test]
fn test_merge_into_base(cx: &mut TestAppContext) {
    cx.update(|cx| init_settings(cx, |_| {}));

    let base = cx.new(|cx| Buffer::local("abcdefghijk", cx));
    let branch = base.update(cx, |buffer, cx| buffer.branch(cx));

    // Make 3 edits, merge one into the base.
    branch.update(cx, |branch, cx| {
        branch.edit([(0..3, "ABC"), (7..9, "HI"), (11..11, "LMN")], None, cx);
        branch.merge_into_base(vec![5..8], cx);
    });

    branch.read_with(cx, |branch, _| assert_eq!(branch.text(), "ABCdefgHIjkLMN"));
    base.read_with(cx, |base, _| assert_eq!(base.text(), "abcdefgHIjk"));

    // Undo the one already-merged edit. Merge that into the base.
    branch.update(cx, |branch, cx| {
        branch.edit([(7..9, "hi")], None, cx);
        branch.merge_into_base(vec![5..8], cx);
    });
    base.read_with(cx, |base, _| assert_eq!(base.text(), "abcdefghijk"));

    // Merge an insertion into the base.
    branch.update(cx, |branch, cx| {
        branch.merge_into_base(vec![11..11], cx);
    });

    branch.read_with(cx, |branch, _| assert_eq!(branch.text(), "ABCdefghijkLMN"));
    base.read_with(cx, |base, _| assert_eq!(base.text(), "abcdefghijkLMN"));

    // Deleted the inserted text and merge that into the base.
    branch.update(cx, |branch, cx| {
        branch.edit([(11..14, "")], None, cx);
        branch.merge_into_base(vec![10..11], cx);
    });

    base.read_with(cx, |base, _| assert_eq!(base.text(), "abcdefghijk"));
}

#[gpui::test]
fn test_undo_after_merge_into_base(cx: &mut TestAppContext) {
    cx.update(|cx| init_settings(cx, |_| {}));

    let base = cx.new(|cx| Buffer::local("abcdefghijk", cx));
    let branch = base.update(cx, |buffer, cx| buffer.branch(cx));

    // Make 2 edits, merge one into the base.
    branch.update(cx, |branch, cx| {
        branch.edit([(0..3, "ABC"), (7..9, "HI")], None, cx);
        branch.merge_into_base(vec![7..7], cx);
    });
    base.read_with(cx, |base, _| assert_eq!(base.text(), "abcdefgHIjk"));
    branch.read_with(cx, |branch, _| assert_eq!(branch.text(), "ABCdefgHIjk"));

    // Undo the merge in the base buffer.
    base.update(cx, |base, cx| {
        base.undo(cx);
    });
    base.read_with(cx, |base, _| assert_eq!(base.text(), "abcdefghijk"));
    branch.read_with(cx, |branch, _| assert_eq!(branch.text(), "ABCdefgHIjk"));

    // Merge that operation into the base again.
    branch.update(cx, |branch, cx| {
        branch.merge_into_base(vec![7..7], cx);
    });
    base.read_with(cx, |base, _| assert_eq!(base.text(), "abcdefgHIjk"));
    branch.read_with(cx, |branch, _| assert_eq!(branch.text(), "ABCdefgHIjk"));
}

#[gpui::test]
async fn test_preview_edits(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init_settings(cx, |_| {});
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });

    let insertion_style = HighlightStyle {
        background_color: Some(cx.read(|cx| cx.theme().status().created_background)),
        ..Default::default()
    };
    let deletion_style = HighlightStyle {
        background_color: Some(cx.read(|cx| cx.theme().status().deleted_background)),
        ..Default::default()
    };

    // no edits
    assert_preview_edits(
        indoc! {"
        fn test_empty() -> bool {
            false
        }"
        },
        vec![],
        true,
        cx,
        |hl| {
            assert!(hl.text.is_empty());
            assert!(hl.highlights.is_empty());
        },
    )
    .await;

    // only insertions
    assert_preview_edits(
        indoc! {"
        fn calculate_area(: f64) -> f64 {
            std::f64::consts::PI * .powi(2)
        }"
        },
        vec![
            (Point::new(0, 18)..Point::new(0, 18), "radius"),
            (Point::new(1, 27)..Point::new(1, 27), "radius"),
        ],
        true,
        cx,
        |hl| {
            assert_eq!(
                hl.text,
                indoc! {"
                fn calculate_area(radius: f64) -> f64 {
                    std::f64::consts::PI * radius.powi(2)"
                }
            );

            assert_eq!(hl.highlights.len(), 2);
            assert_eq!(hl.highlights[0], ((18..24), insertion_style));
            assert_eq!(hl.highlights[1], ((67..73), insertion_style));
        },
    )
    .await;

    // insertions & deletions
    assert_preview_edits(
        indoc! {"
        struct Person {
            first_name: String,
        }

        impl Person {
            fn first_name(&self) -> &String {
                &self.first_name
            }
        }"
        },
        vec![
            (Point::new(1, 4)..Point::new(1, 9), "last"),
            (Point::new(5, 7)..Point::new(5, 12), "last"),
            (Point::new(6, 14)..Point::new(6, 19), "last"),
        ],
        true,
        cx,
        |hl| {
            assert_eq!(
                hl.text,
                indoc! {"
                        firstlast_name: String,
                    }

                    impl Person {
                        fn firstlast_name(&self) -> &String {
                            &self.firstlast_name"
                }
            );

            assert_eq!(hl.highlights.len(), 6);
            assert_eq!(hl.highlights[0], ((4..9), deletion_style));
            assert_eq!(hl.highlights[1], ((9..13), insertion_style));
            assert_eq!(hl.highlights[2], ((52..57), deletion_style));
            assert_eq!(hl.highlights[3], ((57..61), insertion_style));
            assert_eq!(hl.highlights[4], ((101..106), deletion_style));
            assert_eq!(hl.highlights[5], ((106..110), insertion_style));
        },
    )
    .await;

    async fn assert_preview_edits(
        text: &str,
        edits: Vec<(Range<Point>, &str)>,
        include_deletions: bool,
        cx: &mut TestAppContext,
        assert_fn: impl Fn(HighlightedText),
    ) {
        let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(rust_lang(), cx));
        let edits = buffer.read_with(cx, |buffer, _| {
            edits
                .into_iter()
                .map(|(range, text)| {
                    (
                        buffer.anchor_before(range.start)..buffer.anchor_after(range.end),
                        text.into(),
                    )
                })
                .collect::<Arc<[_]>>()
        });
        let edit_preview = buffer
            .read_with(cx, |buffer, cx| buffer.preview_edits(edits.clone(), cx))
            .await;
        let highlighted_edits = cx.read(|cx| {
            edit_preview.highlight_edits(&buffer.read(cx).snapshot(), &edits, include_deletions, cx)
        });
        assert_fn(highlighted_edits);
    }
}

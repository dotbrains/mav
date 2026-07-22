use super::*;
use crate::{MultiBuffer, ToPoint, display_map::inlay_map::InlayMap};
use sum_tree::Bias::{Left, Right};
use util::test::sample_text;

#[gpui::test]
fn test_basic_folds(cx: &mut gpui::App) {
    init_test(cx);
    let buffer = MultiBuffer::build_simple(&sample_text(5, 6, 'a'), cx);
    let subscription = buffer.update(cx, |buffer, _| buffer.subscribe());
    let buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot);
    let mut map = FoldMap::new(inlay_snapshot.clone()).0;

    let (mut writer, _, _) = map.write(inlay_snapshot, vec![]);
    let (snapshot2, edits) = writer.fold(vec![
        (Point::new(0, 2)..Point::new(2, 2), FoldPlaceholder::test()),
        (Point::new(2, 4)..Point::new(4, 1), FoldPlaceholder::test()),
    ]);
    assert_eq!(snapshot2.text(), "aa⋯cc⋯eeeee");
    assert_eq!(
        edits,
        &[
            FoldEdit {
                old: FoldOffset(MultiBufferOffset(2))..FoldOffset(MultiBufferOffset(16)),
                new: FoldOffset(MultiBufferOffset(2))..FoldOffset(MultiBufferOffset(5)),
            },
            FoldEdit {
                old: FoldOffset(MultiBufferOffset(18))..FoldOffset(MultiBufferOffset(29)),
                new: FoldOffset(MultiBufferOffset(7))..FoldOffset(MultiBufferOffset(10)),
            },
        ]
    );

    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit(
            vec![
                (Point::new(0, 0)..Point::new(0, 1), "123"),
                (Point::new(2, 3)..Point::new(2, 3), "123"),
            ],
            None,
            cx,
        );
        buffer.snapshot(cx)
    });

    let (inlay_snapshot, inlay_edits) =
        inlay_map.sync(buffer_snapshot, subscription.consume().into_inner());
    let (snapshot3, edits) = map.read(inlay_snapshot, inlay_edits);
    assert_eq!(snapshot3.text(), "123a⋯c123c⋯eeeee");
    assert_eq!(
        edits,
        &[
            FoldEdit {
                old: FoldOffset(MultiBufferOffset(0))..FoldOffset(MultiBufferOffset(1)),
                new: FoldOffset(MultiBufferOffset(0))..FoldOffset(MultiBufferOffset(3)),
            },
            FoldEdit {
                old: FoldOffset(MultiBufferOffset(6))..FoldOffset(MultiBufferOffset(6)),
                new: FoldOffset(MultiBufferOffset(8))..FoldOffset(MultiBufferOffset(11)),
            },
        ]
    );

    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(2, 6)..Point::new(4, 3), "456")], None, cx);
        buffer.snapshot(cx)
    });
    let (inlay_snapshot, inlay_edits) =
        inlay_map.sync(buffer_snapshot, subscription.consume().into_inner());
    let (snapshot4, _) = map.read(inlay_snapshot.clone(), inlay_edits);
    assert_eq!(snapshot4.text(), "123a⋯c123456eee");

    let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
    writer.unfold_intersecting(Some(Point::new(0, 4)..Point::new(0, 4)), false);
    let (snapshot5, _) = map.read(inlay_snapshot.clone(), vec![]);
    assert_eq!(snapshot5.text(), "123a⋯c123456eee");

    let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
    writer.unfold_intersecting(Some(Point::new(0, 4)..Point::new(0, 4)), true);
    let (snapshot6, _) = map.read(inlay_snapshot, vec![]);
    assert_eq!(snapshot6.text(), "123aaaaa\nbbbbbb\nccc123456eee");
}

#[gpui::test]
fn test_adjacent_folds(cx: &mut gpui::App) {
    init_test(cx);
    let buffer = MultiBuffer::build_simple("abcdefghijkl", cx);
    let subscription = buffer.update(cx, |buffer, _| buffer.subscribe());
    let buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot);

    {
        let mut map = FoldMap::new(inlay_snapshot.clone()).0;

        let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
        writer.fold(vec![(
            MultiBufferOffset(5)..MultiBufferOffset(8),
            FoldPlaceholder::test(),
        )]);
        let (snapshot, _) = map.read(inlay_snapshot.clone(), vec![]);
        assert_eq!(snapshot.text(), "abcde⋯ijkl");

        // Create an fold adjacent to the start of the first fold.
        let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
        writer.fold(vec![
            (
                MultiBufferOffset(0)..MultiBufferOffset(1),
                FoldPlaceholder::test(),
            ),
            (
                MultiBufferOffset(2)..MultiBufferOffset(5),
                FoldPlaceholder::test(),
            ),
        ]);
        let (snapshot, _) = map.read(inlay_snapshot.clone(), vec![]);
        assert_eq!(snapshot.text(), "⋯b⋯ijkl");

        // Create an fold adjacent to the end of the first fold.
        let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
        writer.fold(vec![
            (
                MultiBufferOffset(11)..MultiBufferOffset(11),
                FoldPlaceholder::test(),
            ),
            (
                MultiBufferOffset(8)..MultiBufferOffset(10),
                FoldPlaceholder::test(),
            ),
        ]);
        let (snapshot, _) = map.read(inlay_snapshot.clone(), vec![]);
        assert_eq!(snapshot.text(), "⋯b⋯kl");
    }

    {
        let mut map = FoldMap::new(inlay_snapshot.clone()).0;

        // Create two adjacent folds.
        let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
        writer.fold(vec![
            (
                MultiBufferOffset(0)..MultiBufferOffset(2),
                FoldPlaceholder::test(),
            ),
            (
                MultiBufferOffset(2)..MultiBufferOffset(5),
                FoldPlaceholder::test(),
            ),
        ]);
        let (snapshot, _) = map.read(inlay_snapshot, vec![]);
        assert_eq!(snapshot.text(), "⋯fghijkl");

        // Edit within one of the folds.
        let buffer_snapshot = buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [(MultiBufferOffset(0)..MultiBufferOffset(1), "12345")],
                None,
                cx,
            );
            buffer.snapshot(cx)
        });
        let (inlay_snapshot, inlay_edits) =
            inlay_map.sync(buffer_snapshot, subscription.consume().into_inner());
        let (snapshot, _) = map.read(inlay_snapshot, inlay_edits);
        assert_eq!(snapshot.text(), "12345⋯fghijkl");
    }
}

#[gpui::test]
fn test_overlapping_folds(cx: &mut gpui::App) {
    let buffer = MultiBuffer::build_simple(&sample_text(5, 6, 'a'), cx);
    let buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot);
    let mut map = FoldMap::new(inlay_snapshot.clone()).0;
    let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
    writer.fold(vec![
        (Point::new(0, 2)..Point::new(2, 2), FoldPlaceholder::test()),
        (Point::new(0, 4)..Point::new(1, 0), FoldPlaceholder::test()),
        (Point::new(1, 2)..Point::new(3, 2), FoldPlaceholder::test()),
        (Point::new(3, 1)..Point::new(4, 1), FoldPlaceholder::test()),
    ]);
    let (snapshot, _) = map.read(inlay_snapshot, vec![]);
    assert_eq!(snapshot.text(), "aa⋯eeeee");
}

#[gpui::test]
fn test_merging_folds_via_edit(cx: &mut gpui::App) {
    init_test(cx);
    let buffer = MultiBuffer::build_simple(&sample_text(5, 6, 'a'), cx);
    let subscription = buffer.update(cx, |buffer, _| buffer.subscribe());
    let buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot);
    let mut map = FoldMap::new(inlay_snapshot.clone()).0;

    let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
    writer.fold(vec![
        (Point::new(0, 2)..Point::new(2, 2), FoldPlaceholder::test()),
        (Point::new(3, 1)..Point::new(4, 1), FoldPlaceholder::test()),
    ]);
    let (snapshot, _) = map.read(inlay_snapshot, vec![]);
    assert_eq!(snapshot.text(), "aa⋯cccc\nd⋯eeeee");

    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(2, 2)..Point::new(3, 1), "")], None, cx);
        buffer.snapshot(cx)
    });
    let (inlay_snapshot, inlay_edits) =
        inlay_map.sync(buffer_snapshot, subscription.consume().into_inner());
    let (snapshot, _) = map.read(inlay_snapshot, inlay_edits);
    assert_eq!(snapshot.text(), "aa⋯eeeee");
}

#[gpui::test]
fn test_folds_in_range(cx: &mut gpui::App) {
    let buffer = MultiBuffer::build_simple(&sample_text(5, 6, 'a'), cx);
    let buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let mut map = FoldMap::new(inlay_snapshot.clone()).0;

    let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
    writer.fold(vec![
        (Point::new(0, 2)..Point::new(2, 2), FoldPlaceholder::test()),
        (Point::new(0, 4)..Point::new(1, 0), FoldPlaceholder::test()),
        (Point::new(1, 2)..Point::new(3, 2), FoldPlaceholder::test()),
        (Point::new(3, 1)..Point::new(4, 1), FoldPlaceholder::test()),
    ]);
    let (snapshot, _) = map.read(inlay_snapshot, vec![]);
    let fold_ranges = snapshot
        .folds_in_range(Point::new(1, 0)..Point::new(1, 3))
        .map(|fold| {
            fold.range.start.to_point(&buffer_snapshot)..fold.range.end.to_point(&buffer_snapshot)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fold_ranges,
        vec![
            Point::new(0, 2)..Point::new(2, 2),
            Point::new(1, 2)..Point::new(3, 2)
        ]
    );
}

#[gpui::test]
fn test_buffer_rows(cx: &mut gpui::App) {
    let text = sample_text(6, 6, 'a') + "\n";
    let buffer = MultiBuffer::build_simple(&text, cx);

    let buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot);
    let mut map = FoldMap::new(inlay_snapshot.clone()).0;

    let (mut writer, _, _) = map.write(inlay_snapshot.clone(), vec![]);
    writer.fold(vec![
        (Point::new(0, 2)..Point::new(2, 2), FoldPlaceholder::test()),
        (Point::new(3, 1)..Point::new(4, 1), FoldPlaceholder::test()),
    ]);

    let (snapshot, _) = map.read(inlay_snapshot, vec![]);
    assert_eq!(snapshot.text(), "aa⋯cccc\nd⋯eeeee\nffffff\n");
    assert_eq!(
        snapshot
            .row_infos(0)
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        [Some(0), Some(3), Some(5), Some(6)]
    );
    assert_eq!(
        snapshot
            .row_infos(3)
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        [Some(6)]
    );
}

#[gpui::test(iterations = 100)]
fn test_random_chunk_bitmaps(cx: &mut gpui::App, mut rng: StdRng) {
    init_test(cx);

    // Generate random buffer using existing test infrastructure
    let text_len = rng.random_range(0..10000);
    let buffer = if rng.random() {
        let text = RandomCharIter::new(&mut rng)
            .take(text_len)
            .collect::<String>();
        MultiBuffer::build_simple(&text, cx)
    } else {
        MultiBuffer::build_random(&mut rng, cx)
    };
    let buffer_snapshot = buffer.read(cx).snapshot(cx);
    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot);
    let (mut fold_map, _) = FoldMap::new(inlay_snapshot.clone());

    // Perform random mutations
    let mutation_count = rng.random_range(1..10);
    for _ in 0..mutation_count {
        fold_map.randomly_mutate(&mut rng);
    }

    let (snapshot, _) = fold_map.read(inlay_snapshot, vec![]);

    // Get all chunks and verify their bitmaps
    let chunks = snapshot.chunks(
        FoldOffset(MultiBufferOffset(0))..FoldOffset(snapshot.len().0),
        LanguageAwareStyling {
            tree_sitter: false,
            diagnostics: false,
        },
        Highlights::default(),
    );

    for chunk in chunks {
        let chunk_text = chunk.text;
        let chars_bitmap = chunk.chars;
        let tabs_bitmap = chunk.tabs;

        // Check empty chunks have empty bitmaps
        if chunk_text.is_empty() {
            assert_eq!(
                chars_bitmap, 0,
                "Empty chunk should have empty chars bitmap"
            );
            assert_eq!(tabs_bitmap, 0, "Empty chunk should have empty tabs bitmap");
            continue;
        }

        // Verify that chunk text doesn't exceed 128 bytes
        assert!(
            chunk_text.len() <= 128,
            "Chunk text length {} exceeds 128 bytes",
            chunk_text.len()
        );

        // Verify chars bitmap
        let char_indices = chunk_text
            .char_indices()
            .map(|(i, _)| i)
            .collect::<Vec<_>>();

        for byte_idx in 0..chunk_text.len() {
            let should_have_bit = char_indices.contains(&byte_idx);
            let has_bit = chars_bitmap & (1 << byte_idx) != 0;

            if has_bit != should_have_bit {
                eprintln!("Chunk text bytes: {:?}", chunk_text.as_bytes());
                eprintln!("Char indices: {:?}", char_indices);
                eprintln!("Chars bitmap: {:#b}", chars_bitmap);
                assert_eq!(
                    has_bit, should_have_bit,
                    "Chars bitmap mismatch at byte index {} in chunk {:?}. Expected bit: {}, Got bit: {}",
                    byte_idx, chunk_text, should_have_bit, has_bit
                );
            }
        }

        // Verify tabs bitmap
        for (byte_idx, byte) in chunk_text.bytes().enumerate() {
            let is_tab = byte == b'\t';
            let has_bit = tabs_bitmap & (1 << byte_idx) != 0;

            assert_eq!(
                has_bit, is_tab,
                "Tabs bitmap mismatch at byte index {} in chunk {:?}. Byte: {:?}, Expected bit: {}, Got bit: {}",
                byte_idx, chunk_text, byte as char, is_tab, has_bit
            );
        }
    }
}

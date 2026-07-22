use super::*;

#[gpui::test(iterations = 100)]
fn test_random_inlays(cx: &mut App, mut rng: StdRng) {
    init_test(cx);

    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let len = rng.random_range(0..30);
    let buffer = if rng.random() {
        let text = util::RandomCharIter::new(&mut rng)
            .take(len)
            .collect::<String>();
        MultiBuffer::build_simple(&text, cx)
    } else {
        MultiBuffer::build_random(&mut rng, cx)
    };
    let mut buffer_snapshot = buffer.read(cx).snapshot(cx);
    let mut next_inlay_id = 0;
    log::info!("buffer text: {:?}", buffer_snapshot.text());
    let (mut inlay_map, mut inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    for _ in 0..operations {
        let mut inlay_edits = Patch::default();

        let mut prev_inlay_text = inlay_snapshot.text();
        let mut buffer_edits = Vec::new();
        match rng.random_range(0..=100) {
            0..=50 => {
                let (snapshot, edits) = inlay_map.randomly_mutate(&mut next_inlay_id, &mut rng);
                log::info!("mutated text: {:?}", snapshot.text());
                inlay_edits = Patch::new(edits);
            }
            _ => buffer.update(cx, |buffer, cx| {
                let subscription = buffer.subscribe();
                let edit_count = rng.random_range(1..=5);
                buffer.randomly_mutate(&mut rng, edit_count, cx);
                buffer_snapshot = buffer.snapshot(cx);
                let edits = subscription.consume().into_inner();
                log::info!("editing {:?}", edits);
                buffer_edits.extend(edits);
            }),
        };

        let (new_inlay_snapshot, new_inlay_edits) =
            inlay_map.sync(buffer_snapshot.clone(), buffer_edits);
        inlay_snapshot = new_inlay_snapshot;
        inlay_edits = inlay_edits.compose(new_inlay_edits);

        log::info!("buffer text: {:?}", buffer_snapshot.text());
        log::info!("inlay text: {:?}", inlay_snapshot.text());

        let inlays = inlay_map
            .inlays
            .iter()
            .filter(|inlay| inlay.position.is_valid(&buffer_snapshot))
            .map(|inlay| {
                let offset = inlay.position.to_offset(&buffer_snapshot);
                (offset, inlay.clone())
            })
            .collect::<Vec<_>>();
        let mut expected_text = Rope::from(&buffer_snapshot.text());
        for (offset, inlay) in inlays.iter().rev() {
            expected_text.replace(offset.0..offset.0, &inlay.text().to_string());
        }
        assert_eq!(inlay_snapshot.text(), expected_text.to_string());

        let expected_buffer_rows = inlay_snapshot.row_infos(0).collect::<Vec<_>>();
        assert_eq!(
            expected_buffer_rows.len() as u32,
            expected_text.max_point().row + 1
        );
        for row_start in 0..expected_buffer_rows.len() {
            assert_eq!(
                inlay_snapshot
                    .row_infos(row_start as u32)
                    .collect::<Vec<_>>(),
                &expected_buffer_rows[row_start..],
                "incorrect buffer rows starting at {}",
                row_start
            );
        }

        let mut text_highlights = HashMap::default();
        let text_highlight_count = rng.random_range(0_usize..10);
        let mut text_highlight_ranges = (0..text_highlight_count)
            .map(|_| buffer_snapshot.random_byte_range(MultiBufferOffset(0), &mut rng))
            .collect::<Vec<_>>();
        text_highlight_ranges.sort_by_key(|range| (range.start, Reverse(range.end)));
        log::info!("highlighting text ranges {text_highlight_ranges:?}");
        text_highlights.insert(
            HighlightKey::ColorizeBracket(0),
            Arc::new((
                HighlightStyle::default(),
                text_highlight_ranges
                    .into_iter()
                    .map(|range| {
                        buffer_snapshot.anchor_before(range.start)
                            ..buffer_snapshot.anchor_after(range.end)
                    })
                    .collect(),
            )),
        );
        let text_highlights = Arc::new(text_highlights);

        let mut inlay_highlights = InlayHighlights::default();
        if !inlays.is_empty() {
            let inlay_highlight_count = rng.random_range(0..inlays.len());
            let mut inlay_indices = BTreeSet::default();
            while inlay_indices.len() < inlay_highlight_count {
                inlay_indices.insert(rng.random_range(0..inlays.len()));
            }
            let new_highlights = TreeMap::from_ordered_entries(
                inlay_indices
                    .into_iter()
                    .filter_map(|i| {
                        let (_, inlay) = &inlays[i];
                        let inlay_text_len = inlay.text().len();
                        match inlay_text_len {
                            0 => None,
                            1 => Some(InlayHighlight {
                                inlay: inlay.id,
                                inlay_position: inlay.position,
                                range: 0..1,
                            }),
                            n => {
                                let inlay_text = inlay.text().to_string();
                                let mut highlight_end = rng.random_range(1..n);
                                let mut highlight_start = rng.random_range(0..highlight_end);
                                while !inlay_text.is_char_boundary(highlight_end) {
                                    highlight_end += 1;
                                }
                                while !inlay_text.is_char_boundary(highlight_start) {
                                    highlight_start -= 1;
                                }
                                Some(InlayHighlight {
                                    inlay: inlay.id,
                                    inlay_position: inlay.position,
                                    range: highlight_start..highlight_end,
                                })
                            }
                        }
                    })
                    .map(|highlight| (highlight.inlay, (HighlightStyle::default(), highlight))),
            );
            log::info!("highlighting inlay ranges {new_highlights:?}");
            inlay_highlights.insert(HighlightKey::Editor, new_highlights);
        }

        for _ in 0..5 {
            let mut end = rng.random_range(0..=inlay_snapshot.len().0.0);
            end = expected_text.clip_offset(end, Bias::Right);
            let mut start = rng.random_range(0..=end);
            start = expected_text.clip_offset(start, Bias::Right);

            let range = InlayOffset(MultiBufferOffset(start))..InlayOffset(MultiBufferOffset(end));
            log::info!("calling inlay_snapshot.chunks({range:?})");
            let actual_text = inlay_snapshot
                .chunks(
                    range,
                    LanguageAwareStyling {
                        tree_sitter: false,
                        diagnostics: false,
                    },
                    Highlights {
                        text_highlights: Some(&text_highlights),
                        inlay_highlights: Some(&inlay_highlights),
                        ..Highlights::default()
                    },
                )
                .map(|chunk| chunk.chunk.text)
                .collect::<String>();
            assert_eq!(
                actual_text,
                expected_text.slice(start..end).to_string(),
                "incorrect text in range {:?}",
                start..end
            );

            assert_eq!(
                inlay_snapshot.text_summary_for_range(
                    InlayOffset(MultiBufferOffset(start))..InlayOffset(MultiBufferOffset(end))
                ),
                MBTextSummary::from(expected_text.slice(start..end).summary())
            );
        }

        for edit in inlay_edits {
            prev_inlay_text.replace_range(
                edit.new.start.0.0..edit.new.start.0.0 + edit.old_len(),
                &inlay_snapshot.text()[edit.new.start.0.0..edit.new.end.0.0],
            );
        }
        assert_eq!(prev_inlay_text, inlay_snapshot.text());

        assert_eq!(expected_text.max_point(), inlay_snapshot.max_point().0);
        assert_eq!(expected_text.len(), inlay_snapshot.len().0.0);

        let mut buffer_point = Point::default();
        let mut inlay_point = inlay_snapshot.to_inlay_point(buffer_point);
        let mut buffer_chars = buffer_snapshot.chars_at(MultiBufferOffset(0));
        loop {
            // Ensure conversion from buffer coordinates to inlay coordinates
            // is consistent.
            let buffer_offset = buffer_snapshot.point_to_offset(buffer_point);
            assert_eq!(
                inlay_snapshot.to_point(inlay_snapshot.to_inlay_offset(buffer_offset)),
                inlay_point
            );

            // No matter which bias we clip an inlay point with, it doesn't move
            // because it was constructed from a buffer point.
            assert_eq!(
                inlay_snapshot.clip_point(inlay_point, Bias::Left),
                inlay_point,
                "invalid inlay point for buffer point {:?} when clipped left",
                buffer_point
            );
            assert_eq!(
                inlay_snapshot.clip_point(inlay_point, Bias::Right),
                inlay_point,
                "invalid inlay point for buffer point {:?} when clipped right",
                buffer_point
            );

            if let Some(ch) = buffer_chars.next() {
                if ch == '\n' {
                    buffer_point += Point::new(1, 0);
                } else {
                    buffer_point += Point::new(0, ch.len_utf8() as u32);
                }

                // Ensure that moving forward in the buffer always moves the inlay point forward as well.
                let new_inlay_point = inlay_snapshot.to_inlay_point(buffer_point);
                assert!(new_inlay_point > inlay_point);
                inlay_point = new_inlay_point;
            } else {
                break;
            }
        }

        let mut inlay_point = InlayPoint::default();
        let mut inlay_offset = InlayOffset::default();
        for ch in expected_text.chars() {
            assert_eq!(
                inlay_snapshot.to_offset(inlay_point),
                inlay_offset,
                "invalid to_offset({:?})",
                inlay_point
            );
            assert_eq!(
                inlay_snapshot.to_point(inlay_offset),
                inlay_point,
                "invalid to_point({:?})",
                inlay_offset
            );

            let mut bytes = [0; 4];
            for byte in ch.encode_utf8(&mut bytes).as_bytes() {
                inlay_offset.0 += 1;
                if *byte == b'\n' {
                    inlay_point.0 += Point::new(1, 0);
                } else {
                    inlay_point.0 += Point::new(0, 1);
                }

                let clipped_left_point = inlay_snapshot.clip_point(inlay_point, Bias::Left);
                let clipped_right_point = inlay_snapshot.clip_point(inlay_point, Bias::Right);
                assert!(
                    clipped_left_point <= clipped_right_point,
                    "inlay point {:?} when clipped left is greater than when clipped right ({:?} > {:?})",
                    inlay_point,
                    clipped_left_point,
                    clipped_right_point
                );

                // Ensure the clipped points are at valid text locations.
                assert_eq!(
                    clipped_left_point.0,
                    expected_text.clip_point(clipped_left_point.0, Bias::Left)
                );
                assert_eq!(
                    clipped_right_point.0,
                    expected_text.clip_point(clipped_right_point.0, Bias::Right)
                );

                // Ensure the clipped points never overshoot the end of the map.
                assert!(clipped_left_point <= inlay_snapshot.max_point());
                assert!(clipped_right_point <= inlay_snapshot.max_point());

                // Ensure the clipped points are at valid buffer locations.
                assert_eq!(
                    inlay_snapshot
                        .to_inlay_point(inlay_snapshot.to_buffer_point(clipped_left_point)),
                    clipped_left_point,
                    "to_buffer_point({:?}) = {:?}",
                    clipped_left_point,
                    inlay_snapshot.to_buffer_point(clipped_left_point),
                );
                assert_eq!(
                    inlay_snapshot
                        .to_inlay_point(inlay_snapshot.to_buffer_point(clipped_right_point)),
                    clipped_right_point,
                    "to_buffer_point({:?}) = {:?}",
                    clipped_right_point,
                    inlay_snapshot.to_buffer_point(clipped_right_point),
                );
            }
        }
    }
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
    let (mut inlay_map, _) = InlayMap::new(buffer_snapshot.clone());

    // Perform random mutations to add inlays
    let mut next_inlay_id = 0;
    let mutation_count = rng.random_range(1..10);
    for _ in 0..mutation_count {
        inlay_map.randomly_mutate(&mut next_inlay_id, &mut rng);
    }

    let (snapshot, _) = inlay_map.sync(buffer_snapshot, vec![]);

    // Get all chunks and verify their bitmaps
    let chunks = snapshot.chunks(
        InlayOffset(MultiBufferOffset(0))..snapshot.len(),
        LanguageAwareStyling {
            tree_sitter: false,
            diagnostics: false,
        },
        Highlights::default(),
    );

    for chunk in chunks.into_iter().map(|inlay_chunk| inlay_chunk.chunk) {
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

            if has_bit != is_tab {
                eprintln!("Chunk text bytes: {:?}", chunk_text.as_bytes());
                eprintln!("Tabs bitmap: {:#b}", tabs_bitmap);
                assert_eq!(
                    has_bit, is_tab,
                    "Tabs bitmap mismatch at byte index {} in chunk {:?}. Byte: {:?}, Expected bit: {}, Got bit: {}",
                    byte_idx, chunk_text, byte as char, is_tab, has_bit
                );
            }
        }
    }
}

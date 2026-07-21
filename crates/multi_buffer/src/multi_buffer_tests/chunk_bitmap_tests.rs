use super::*;

#[gpui::test(iterations = 100)]
fn test_random_chunk_bitmaps(cx: &mut App, mut rng: StdRng) {
    let multibuffer = if rng.random() {
        let len = rng.random_range(0..10000);
        let text = RandomCharIter::new(&mut rng).take(len).collect::<String>();
        let buffer = cx.new(|cx| Buffer::local(text, cx));
        cx.new(|cx| MultiBuffer::singleton(buffer, cx))
    } else {
        MultiBuffer::build_random(&mut rng, cx)
    };

    let snapshot = multibuffer.read(cx).snapshot(cx);

    let chunks = snapshot.chunks(
        MultiBufferOffset(0)..snapshot.len(),
        LanguageAwareStyling {
            tree_sitter: false,
            diagnostics: false,
        },
    );

    for chunk in chunks {
        let chunk_text = chunk.text;
        let chars_bitmap = chunk.chars;
        let tabs_bitmap = chunk.tabs;

        if chunk_text.is_empty() {
            assert_eq!(
                chars_bitmap, 0,
                "Empty chunk should have empty chars bitmap"
            );
            assert_eq!(tabs_bitmap, 0, "Empty chunk should have empty tabs bitmap");
            continue;
        }

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
            }

            assert_eq!(
                has_bit, should_have_bit,
                "Chars bitmap mismatch at byte index {} in chunk {:?}. Expected bit: {}, Got bit: {}",
                byte_idx, chunk_text, should_have_bit, has_bit
            );
        }

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

#[gpui::test(iterations = 10)]
fn test_random_chunk_bitmaps_with_diffs(cx: &mut App, mut rng: StdRng) {
    let settings_store = SettingsStore::test(cx);
    cx.set_global(settings_store);
    use buffer_diff::BufferDiff;
    use util::RandomCharIter;

    let multibuffer = if rng.random() {
        let len = rng.random_range(100..10000);
        let text = RandomCharIter::new(&mut rng).take(len).collect::<String>();
        let buffer = cx.new(|cx| Buffer::local(text, cx));
        cx.new(|cx| MultiBuffer::singleton(buffer, cx))
    } else {
        MultiBuffer::build_random(&mut rng, cx)
    };

    let _diff_count = rng.random_range(1..5);
    let mut diffs = Vec::new();

    multibuffer.update(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        for buffer_id in snapshot.all_buffer_ids() {
            if rng.random_bool(0.7) {
                if let Some(buffer_handle) = multibuffer.buffer(buffer_id) {
                    let buffer_text = buffer_handle.read(cx).text();
                    let mut base_text = String::new();

                    for line in buffer_text.lines() {
                        if rng.random_bool(0.3) {
                            continue;
                        } else if rng.random_bool(0.3) {
                            let line_len = rng.random_range(0..50);
                            let modified_line = RandomCharIter::new(&mut rng)
                                .take(line_len)
                                .collect::<String>();
                            base_text.push_str(&modified_line);
                            base_text.push('\n');
                        } else {
                            base_text.push_str(line);
                            base_text.push('\n');
                        }
                    }

                    if rng.random_bool(0.5) {
                        let extra_lines = rng.random_range(1..5);
                        for _ in 0..extra_lines {
                            let line_len = rng.random_range(0..50);
                            let extra_line = RandomCharIter::new(&mut rng)
                                .take(line_len)
                                .collect::<String>();
                            base_text.push_str(&extra_line);
                            base_text.push('\n');
                        }
                    }

                    let diff = cx.new(|cx| {
                        BufferDiff::new_with_base_text(
                            &base_text,
                            &buffer_handle.read(cx).text_snapshot(),
                            cx,
                        )
                    });
                    diffs.push(diff.clone());
                    multibuffer.add_diff(diff, cx);
                }
            }
        }
    });

    multibuffer.update(cx, |multibuffer, cx| {
        if rng.random_bool(0.5) {
            multibuffer.set_all_diff_hunks_expanded(cx);
        } else {
            let snapshot = multibuffer.snapshot(cx);
            let text = snapshot.text();

            let mut ranges = Vec::new();
            for _ in 0..rng.random_range(1..5) {
                if snapshot.len().0 == 0 {
                    break;
                }

                let diff_size = rng.random_range(5..1000);
                let mut start = rng.random_range(0..snapshot.len().0);

                while !text.is_char_boundary(start) {
                    start = start.saturating_sub(1);
                }

                let mut end = rng.random_range(start..snapshot.len().0.min(start + diff_size));

                while !text.is_char_boundary(end) {
                    end = end.saturating_add(1);
                }
                let start_anchor = snapshot.anchor_after(MultiBufferOffset(start));
                let end_anchor = snapshot.anchor_before(MultiBufferOffset(end));
                ranges.push(start_anchor..end_anchor);
            }
            multibuffer.expand_diff_hunks(ranges, cx);
        }
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);

    let chunks = snapshot.chunks(
        MultiBufferOffset(0)..snapshot.len(),
        LanguageAwareStyling {
            tree_sitter: false,
            diagnostics: false,
        },
    );

    for chunk in chunks {
        let chunk_text = chunk.text;
        let chars_bitmap = chunk.chars;
        let tabs_bitmap = chunk.tabs;

        if chunk_text.is_empty() {
            assert_eq!(
                chars_bitmap, 0,
                "Empty chunk should have empty chars bitmap"
            );
            assert_eq!(tabs_bitmap, 0, "Empty chunk should have empty tabs bitmap");
            continue;
        }

        assert!(
            chunk_text.len() <= 128,
            "Chunk text length {} exceeds 128 bytes",
            chunk_text.len()
        );

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
            }

            assert_eq!(
                has_bit, should_have_bit,
                "Chars bitmap mismatch at byte index {} in chunk {:?}. Expected bit: {}, Got bit: {}",
                byte_idx, chunk_text, should_have_bit, has_bit
            );
        }

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

use super::*;

pub(super) fn assert_random_block_snapshot(
    rng: &mut StdRng,
    blocks_snapshot: &BlockMapReader<'_>,
    buffer_snapshot: &MultiBufferSnapshot,
    expected_text: &str,
    expected_buffer_rows: &[Option<MultiBufferRow>],
    expected_block_positions: Vec<(u32, BlockId)>,
    expected_replaced_buffer_rows: &HashSet<MultiBufferRow>,
) {
    let expected_lines = expected_text.split('\n').collect::<Vec<_>>();
    let expected_row_count = expected_lines.len();
    log::info!("expected text: {expected_text:?}");

    assert_eq!(
        blocks_snapshot.max_point().row + 1,
        expected_row_count as u32,
        "actual row count != expected row count",
    );
    assert_eq!(
        blocks_snapshot.text(),
        expected_text,
        "actual text != expected text",
    );

    for start_row in 0..expected_row_count {
        let end_row = rng.random_range(start_row + 1..=expected_row_count);
        let mut expected_text = expected_lines[start_row..end_row].join("\n");
        if end_row < expected_row_count {
            expected_text.push('\n');
        }

        let actual_text = blocks_snapshot
            .chunks(
                BlockRow(start_row as u32)..BlockRow(end_row as u32),
                LanguageAwareStyling {
                    tree_sitter: false,
                    diagnostics: false,
                },
                false,
                Highlights::default(),
            )
            .map(|chunk| chunk.text)
            .collect::<String>();
        assert_eq!(
            actual_text,
            expected_text,
            "incorrect text starting row row range {:?}",
            start_row..end_row
        );
        assert_eq!(
            blocks_snapshot
                .row_infos(BlockRow(start_row as u32))
                .map(|row_info| row_info.buffer_row)
                .collect::<Vec<_>>(),
            &expected_buffer_rows[start_row..],
            "incorrect buffer_rows starting at row {:?}",
            start_row
        );
    }

    assert_eq!(
        blocks_snapshot
            .blocks_in_range(BlockRow(0)..BlockRow(expected_row_count as u32))
            .map(|(row, block)| (row.0, block.id()))
            .collect::<Vec<_>>(),
        expected_block_positions,
        "invalid blocks_in_range({:?})",
        0..expected_row_count
    );

    for (_, expected_block) in
        blocks_snapshot.blocks_in_range(BlockRow(0)..BlockRow(expected_row_count as u32))
    {
        let actual_block = blocks_snapshot.block_for_id(expected_block.id());
        assert_eq!(
            actual_block.map(|block| block.id()),
            Some(expected_block.id())
        );
    }

    for (block_row, block_id) in expected_block_positions {
        if let BlockId::Custom(block_id) = block_id {
            assert_eq!(
                blocks_snapshot.row_for_block(block_id),
                Some(BlockRow(block_row))
            );
        }
    }

    let mut expected_longest_rows = Vec::new();
    let mut longest_line_len = -1_isize;
    for (row, line) in expected_lines.iter().enumerate() {
        let row = row as u32;

        assert_eq!(
            blocks_snapshot.line_len(BlockRow(row)),
            line.len() as u32,
            "invalid line len for row {}",
            row
        );

        let line_char_count = line.chars().count() as isize;
        match line_char_count.cmp(&longest_line_len) {
            Ordering::Less => {}
            Ordering::Equal => expected_longest_rows.push(row),
            Ordering::Greater => {
                longest_line_len = line_char_count;
                expected_longest_rows.clear();
                expected_longest_rows.push(row);
            }
        }
    }

    let longest_row = blocks_snapshot.longest_row();
    assert!(
        expected_longest_rows.contains(&longest_row.0),
        "incorrect longest row {}. expected {:?} with length {}",
        longest_row.0,
        expected_longest_rows,
        longest_line_len,
    );

    for _ in 0..10 {
        let end_row = rng.random_range(1..=expected_lines.len());
        let start_row = rng.random_range(0..end_row);

        let mut expected_longest_rows_in_range = vec![];
        let mut longest_line_len_in_range = 0;

        for (row, line) in (start_row as u32..).zip(&expected_lines[start_row..end_row]) {
            let line_char_count = line.chars().count() as isize;
            match line_char_count.cmp(&longest_line_len_in_range) {
                Ordering::Less => {}
                Ordering::Equal => expected_longest_rows_in_range.push(row),
                Ordering::Greater => {
                    longest_line_len_in_range = line_char_count;
                    expected_longest_rows_in_range.clear();
                    expected_longest_rows_in_range.push(row);
                }
            }
        }

        let longest_row_in_range = blocks_snapshot
            .longest_row_in_range(BlockRow(start_row as u32)..BlockRow(end_row as u32));
        assert!(
            expected_longest_rows_in_range.contains(&longest_row_in_range.0),
            "incorrect longest row {} in range {:?}. expected {:?} with length {}",
            longest_row.0,
            start_row..end_row,
            expected_longest_rows_in_range,
            longest_line_len_in_range,
        );
    }

    // Ensure that conversion between block points and wrap points is stable.
    for row in 0..=blocks_snapshot.wrap_snapshot.max_point().row().0 {
        let wrap_point = WrapPoint::new(WrapRow(row), 0);
        let block_point = blocks_snapshot.to_block_point(wrap_point);
        let left_wrap_point = blocks_snapshot.to_wrap_point(block_point, Bias::Left);
        let right_wrap_point = blocks_snapshot.to_wrap_point(block_point, Bias::Right);
        assert_eq!(blocks_snapshot.to_block_point(left_wrap_point), block_point);
        assert_eq!(
            blocks_snapshot.to_block_point(right_wrap_point),
            block_point
        );
    }

    let mut block_point = BlockPoint::new(BlockRow(0), 0);
    for c in expected_text.chars() {
        let left_point = blocks_snapshot.clip_point(block_point, Bias::Left);
        let left_buffer_point = blocks_snapshot.to_point(left_point, Bias::Left);
        assert_eq!(
            blocks_snapshot.to_block_point(blocks_snapshot.to_wrap_point(left_point, Bias::Left)),
            left_point,
            "block point: {:?}, wrap point: {:?}",
            block_point,
            blocks_snapshot.to_wrap_point(left_point, Bias::Left)
        );
        assert_eq!(
            left_buffer_point,
            buffer_snapshot.clip_point(left_buffer_point, Bias::Right),
            "{:?} is not valid in buffer coordinates",
            left_point
        );

        let right_point = blocks_snapshot.clip_point(block_point, Bias::Right);
        let right_buffer_point = blocks_snapshot.to_point(right_point, Bias::Right);
        assert_eq!(
            blocks_snapshot.to_block_point(blocks_snapshot.to_wrap_point(right_point, Bias::Right)),
            right_point,
            "block point: {:?}, wrap point: {:?}",
            block_point,
            blocks_snapshot.to_wrap_point(right_point, Bias::Right)
        );
        assert_eq!(
            right_buffer_point,
            buffer_snapshot.clip_point(right_buffer_point, Bias::Left),
            "{:?} is not valid in buffer coordinates",
            right_point
        );

        if c == '\n' {
            block_point.0 += Point::new(1, 0);
        } else {
            block_point.column += c.len_utf8() as u32;
        }
    }

    for buffer_row in 0..=buffer_snapshot.max_point().row {
        let buffer_row = MultiBufferRow(buffer_row);
        assert_eq!(
            blocks_snapshot.is_line_replaced(buffer_row),
            expected_replaced_buffer_rows.contains(&buffer_row),
            "incorrect is_line_replaced({buffer_row:?}), expected replaced rows: {expected_replaced_buffer_rows:?}",
        );
    }
}

use super::*;

mod random_assertions;

#[gpui::test(iterations = 60)]
fn test_random_blocks(cx: &mut gpui::TestAppContext, mut rng: StdRng) {
    cx.update(init_test);

    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let wrap_width = if rng.random_bool(0.2) {
        None
    } else {
        Some(px(rng.random_range(0.0..=100.0)))
    };
    let tab_size = 1.try_into().unwrap();
    let font_size = px(14.0);
    let buffer_start_header_height = rng.random_range(1..=5);
    let excerpt_header_height = rng.random_range(1..=5);

    log::info!("Wrap width: {:?}", wrap_width);
    log::info!("Excerpt Header Height: {:?}", excerpt_header_height);
    let is_singleton = rng.random();
    let buffer = if is_singleton {
        let len = rng.random_range(0..10);
        let text = RandomCharIter::new(&mut rng).take(len).collect::<String>();
        log::info!("initial singleton buffer text: {:?}", text);
        cx.update(|cx| MultiBuffer::build_simple(&text, cx))
    } else {
        cx.update(|cx| {
            let multibuffer = MultiBuffer::build_random(&mut rng, cx);
            log::info!(
                "initial multi-buffer text: {:?}",
                multibuffer.read(cx).read(cx).text()
            );
            multibuffer
        })
    };

    let mut buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (mut fold_map, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (mut tab_map, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let font = test_font();
    let (wrap_map, wraps_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font, font_size, wrap_width, cx));
    let mut block_map = BlockMap::new(
        wraps_snapshot,
        buffer_start_header_height,
        excerpt_header_height,
    );

    for _ in 0..operations {
        let mut buffer_edits = Vec::new();
        match rng.random_range(0..=100) {
            0..=19 => {
                let wrap_width = if rng.random_bool(0.2) {
                    None
                } else {
                    Some(px(rng.random_range(0.0..=100.0)))
                };
                log::info!("Setting wrap width to {:?}", wrap_width);
                wrap_map.update(cx, |map, cx| map.set_wrap_width(wrap_width, cx));
            }
            20..=39 => {
                let block_count = rng.random_range(1..=5);
                let block_properties = (0..block_count)
                    .map(|_| {
                        let buffer = cx.update(|cx| buffer.read(cx).read(cx).clone());
                        let offset = buffer.clip_offset(
                            rng.random_range(MultiBufferOffset(0)..=buffer.len()),
                            Bias::Left,
                        );
                        let mut min_height = 0;
                        let placement = match rng.random_range(0..3) {
                            0 => {
                                min_height = 1;
                                let start = buffer.anchor_after(offset);
                                let end = buffer.anchor_after(buffer.clip_offset(
                                    rng.random_range(offset..=buffer.len()),
                                    Bias::Left,
                                ));
                                BlockPlacement::Replace(start..=end)
                            }
                            1 => BlockPlacement::Above(buffer.anchor_after(offset)),
                            _ => BlockPlacement::Below(buffer.anchor_after(offset)),
                        };

                        let height = rng.random_range(min_height..512);
                        BlockProperties {
                            style: BlockStyle::Fixed,
                            placement,
                            height: Some(height),
                            render: Arc::new(|_| div().into_any()),
                            priority: 0,
                        }
                    })
                    .collect::<Vec<_>>();

                let (inlay_snapshot, inlay_edits) = inlay_map.sync(buffer_snapshot.clone(), vec![]);
                let (fold_snapshot, fold_edits) = fold_map.read(inlay_snapshot, inlay_edits);
                let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, tab_size);
                let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
                    wrap_map.sync(tab_snapshot, tab_edits, cx)
                });
                let mut block_map = block_map.write(wraps_snapshot, wrap_edits, None);
                let block_ids =
                    block_map.insert(block_properties.iter().map(|props| BlockProperties {
                        placement: props.placement.clone(),
                        height: props.height,
                        style: props.style,
                        render: Arc::new(|_| div().into_any()),
                        priority: 0,
                    }));

                for (block_properties, block_id) in block_properties.iter().zip(block_ids) {
                    log::info!(
                        "inserted block {:?} with height {:?} and id {:?}",
                        block_properties
                            .placement
                            .as_ref()
                            .map(|p| p.to_point(&buffer_snapshot)),
                        block_properties.height,
                        block_id
                    );
                }
            }
            40..=59 if !block_map.custom_blocks.is_empty() => {
                let block_count = rng.random_range(1..=4.min(block_map.custom_blocks.len()));
                let block_ids_to_remove = block_map
                    .custom_blocks
                    .choose_multiple(&mut rng, block_count)
                    .map(|block| block.id)
                    .collect::<HashSet<_>>();

                let (inlay_snapshot, inlay_edits) = inlay_map.sync(buffer_snapshot.clone(), vec![]);
                let (fold_snapshot, fold_edits) = fold_map.read(inlay_snapshot, inlay_edits);
                let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, tab_size);
                let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
                    wrap_map.sync(tab_snapshot, tab_edits, cx)
                });
                let mut block_map = block_map.write(wraps_snapshot, wrap_edits, None);
                log::info!(
                    "removing {} blocks: {:?}",
                    block_ids_to_remove.len(),
                    block_ids_to_remove
                );
                block_map.remove(block_ids_to_remove);
            }
            60..=79 => {
                if buffer.read_with(cx, |buffer, _| buffer.is_singleton()) {
                    log::info!("Noop fold/unfold operation on a singleton buffer");
                    continue;
                }
                let (inlay_snapshot, inlay_edits) = inlay_map.sync(buffer_snapshot.clone(), vec![]);
                let (fold_snapshot, fold_edits) = fold_map.read(inlay_snapshot, inlay_edits);
                let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, tab_size);
                let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
                    wrap_map.sync(tab_snapshot, tab_edits, cx)
                });
                let mut block_map = block_map.write(wraps_snapshot, wrap_edits, None);
                let folded_buffers: Vec<_> =
                    block_map.block_map.folded_buffers.iter().cloned().collect();
                let mut unfolded_buffers = buffer_snapshot
                    .buffer_ids_for_range(Anchor::Min..Anchor::Max)
                    .collect::<Vec<_>>();
                unfolded_buffers.dedup();
                log::debug!("All buffers {unfolded_buffers:?}");
                log::debug!("Folded buffers {folded_buffers:?}");
                unfolded_buffers
                    .retain(|buffer_id| !block_map.block_map.folded_buffers.contains(buffer_id));
                let mut folded_count = folded_buffers.len();
                let mut unfolded_count = unfolded_buffers.len();

                let fold = !unfolded_buffers.is_empty() && rng.random_bool(0.5);
                let unfold = !folded_buffers.is_empty() && rng.random_bool(0.5);
                if !fold && !unfold {
                    log::info!(
                        "Noop fold/unfold operation. Unfolded buffers: {unfolded_count}, folded buffers: {folded_count}"
                    );
                    continue;
                }

                buffer.update(cx, |buffer, cx| {
                    if fold {
                        let buffer_to_fold =
                            unfolded_buffers[rng.random_range(0..unfolded_buffers.len())];
                        log::info!("Folding {buffer_to_fold:?}");
                        let related_excerpts = buffer_snapshot
                            .excerpts()
                            .filter_map(|excerpt| {
                                if excerpt.context.start.buffer_id == buffer_to_fold {
                                    Some((
                                        excerpt.context.start,
                                        buffer_snapshot
                                            .buffer_for_id(buffer_to_fold)
                                            .unwrap()
                                            .text_for_range(excerpt.context)
                                            .collect::<String>(),
                                    ))
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>();
                        log::info!(
                            "Folding {buffer_to_fold:?}, related excerpts: {related_excerpts:?}"
                        );
                        folded_count += 1;
                        unfolded_count -= 1;
                        block_map.fold_buffers([buffer_to_fold], buffer, cx);
                    }
                    if unfold {
                        let buffer_to_unfold =
                            folded_buffers[rng.random_range(0..folded_buffers.len())];
                        log::info!("Unfolding {buffer_to_unfold:?}");
                        unfolded_count += 1;
                        folded_count -= 1;
                        block_map.unfold_buffers([buffer_to_unfold], buffer, cx);
                    }
                    log::info!(
                        "Unfolded buffers: {unfolded_count}, folded buffers: {folded_count}"
                    );
                });
            }
            _ => {
                buffer.update(cx, |buffer, cx| {
                    let mutation_count = rng.random_range(1..=5);
                    let subscription = buffer.subscribe();
                    buffer.randomly_mutate(&mut rng, mutation_count, cx);
                    buffer_snapshot = buffer.snapshot(cx);
                    buffer_edits.extend(subscription.consume());
                    log::info!("buffer text: {:?}", buffer_snapshot.text());
                });
            }
        }

        let (inlay_snapshot, inlay_edits) = inlay_map.sync(buffer_snapshot.clone(), buffer_edits);
        let (fold_snapshot, fold_edits) = fold_map.read(inlay_snapshot, inlay_edits);
        let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, tab_size);
        let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
            wrap_map.sync(tab_snapshot, tab_edits, cx)
        });
        let blocks_snapshot = block_map.read(wraps_snapshot.clone(), wrap_edits, None);
        assert_eq!(
            blocks_snapshot.transforms.summary().input_rows,
            wraps_snapshot.max_point().row() + RowDelta(1)
        );
        log::info!("wrapped text: {:?}", wraps_snapshot.text());
        log::info!("blocks text: {:?}", blocks_snapshot.text());

        let mut expected_blocks = Vec::new();
        expected_blocks.extend(block_map.custom_blocks.iter().filter_map(|block| {
            Some((
                block.placement.to_wrap_row(&wraps_snapshot)?,
                Block::Custom(block.clone()),
            ))
        }));

        let mut inlay_point_cursor = wraps_snapshot.inlay_point_cursor();
        let mut tab_point_cursor = wraps_snapshot.tab_point_cursor();
        let mut fold_point_cursor = wraps_snapshot.fold_point_cursor();
        let mut wrap_point_cursor = wraps_snapshot.wrap_point_cursor();

        // Note that this needs to be synced with the related section in BlockMap::sync
        expected_blocks.extend(block_map.header_and_footer_blocks(
            &buffer_snapshot,
            MultiBufferOffset(0)..,
            |point, bias| {
                wrap_point_cursor
                    .map(
                        tab_point_cursor
                            .map(fold_point_cursor.map(inlay_point_cursor.map(point, bias), bias)),
                    )
                    .row()
            },
        ));

        BlockMap::sort_blocks(&mut expected_blocks);

        for (placement, block) in &expected_blocks {
            log::info!(
                "Block {:?} placement: {:?} Height: {:?}",
                block.id(),
                placement,
                block.height()
            );
        }

        let mut sorted_blocks_iter = expected_blocks.into_iter().peekable();

        let input_buffer_rows = buffer_snapshot
            .row_infos(MultiBufferRow(0))
            .map(|row| row.buffer_row)
            .collect::<Vec<_>>();
        let mut expected_buffer_rows = Vec::new();
        let mut expected_text = String::new();
        let mut expected_block_positions = Vec::new();
        let mut expected_replaced_buffer_rows = HashSet::default();
        let input_text = wraps_snapshot.text();

        // Loop over the input lines, creating (N - 1) empty lines for
        // blocks of height N.
        //
        // It's important to note that output *starts* as one empty line,
        // so we special case row 0 to assume a leading '\n'.
        //
        // Linehood is the birthright of strings.
        let input_text_lines = input_text.split('\n').enumerate().peekable();
        let mut block_row = 0;
        for (wrap_row, input_line) in input_text_lines {
            let wrap_row = WrapRow(wrap_row as u32);
            let multibuffer_row = wraps_snapshot
                .to_point(WrapPoint::new(wrap_row, 0), Bias::Left)
                .row;

            // Create empty lines for the above block
            while let Some((placement, block)) = sorted_blocks_iter.peek() {
                if *placement.start() == wrap_row && block.place_above() {
                    let (_, block) = sorted_blocks_iter.next().unwrap();
                    expected_block_positions.push((block_row, block.id()));
                    if block.height() > 0 {
                        let text = "\n".repeat((block.height() - 1) as usize);
                        if block_row > 0 {
                            expected_text.push('\n')
                        }
                        expected_text.push_str(&text);
                        for _ in 0..block.height() {
                            expected_buffer_rows.push(None);
                        }
                        block_row += block.height();
                    }
                } else {
                    break;
                }
            }

            // Skip lines within replace blocks, then create empty lines for the replace block's height
            let mut is_in_replace_block = false;
            if let Some((BlockPlacement::Replace(replace_range), block)) = sorted_blocks_iter.peek()
                && wrap_row >= *replace_range.start()
            {
                is_in_replace_block = true;

                if wrap_row == *replace_range.start() {
                    if matches!(block, Block::FoldedBuffer { .. }) {
                        expected_buffer_rows.push(None);
                    } else {
                        expected_buffer_rows.push(input_buffer_rows[multibuffer_row as usize]);
                    }
                }

                if wrap_row == *replace_range.end() {
                    expected_block_positions.push((block_row, block.id()));
                    let text = "\n".repeat((block.height() - 1) as usize);
                    if block_row > 0 {
                        expected_text.push('\n');
                    }
                    expected_text.push_str(&text);

                    for _ in 1..block.height() {
                        expected_buffer_rows.push(None);
                    }
                    block_row += block.height();

                    sorted_blocks_iter.next();
                }
            }

            if is_in_replace_block {
                expected_replaced_buffer_rows.insert(MultiBufferRow(multibuffer_row));
            } else {
                let buffer_row = input_buffer_rows[multibuffer_row as usize];
                let soft_wrapped = wraps_snapshot
                    .to_tab_point(WrapPoint::new(wrap_row, 0))
                    .column()
                    > 0;
                expected_buffer_rows.push(if soft_wrapped { None } else { buffer_row });
                if block_row > 0 {
                    expected_text.push('\n');
                }
                expected_text.push_str(input_line);
                block_row += 1;
            }

            while let Some((placement, block)) = sorted_blocks_iter.peek() {
                if *placement.end() == wrap_row && block.place_below() {
                    let (_, block) = sorted_blocks_iter.next().unwrap();
                    expected_block_positions.push((block_row, block.id()));
                    if block.height() > 0 {
                        let text = "\n".repeat((block.height() - 1) as usize);
                        if block_row > 0 {
                            expected_text.push('\n')
                        }
                        expected_text.push_str(&text);
                        for _ in 0..block.height() {
                            expected_buffer_rows.push(None);
                        }
                        block_row += block.height();
                    }
                } else {
                    break;
                }
            }
        }

        random_assertions::assert_random_block_snapshot(
            &mut rng,
            &blocks_snapshot,
            &buffer_snapshot,
            &expected_text,
            &expected_buffer_rows,
            expected_block_positions,
            &expected_replaced_buffer_rows,
        );
    }
}

use super::*;

#[gpui::test(iterations = 100)]
async fn test_random_display_map(cx: &mut gpui::TestAppContext, mut rng: StdRng) {
    cx.background_executor.set_block_on_ticks(0..=50);
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let mut tab_size = rng.random_range(1..=4);
    let buffer_start_excerpt_header_height = rng.random_range(1..=5);
    let excerpt_header_height = rng.random_range(1..=5);
    let font_size = px(14.0);
    let max_wrap_width = 300.0;
    let mut wrap_width = if rng.random_bool(0.1) {
        None
    } else {
        Some(px(rng.random_range(0.0..=max_wrap_width)))
    };

    log::info!("tab size: {}", tab_size);
    log::info!("wrap width: {:?}", wrap_width);

    cx.update(|cx| {
        init_test(cx, &|s| {
            s.project.all_languages.defaults.tab_size = NonZeroU32::new(tab_size)
        });
    });

    let buffer = cx.update(|cx| {
        if rng.random() {
            let len = rng.random_range(0..10);
            let text = util::RandomCharIter::new(&mut rng)
                .take(len)
                .collect::<String>();
            MultiBuffer::build_simple(&text, cx)
        } else {
            MultiBuffer::build_random(&mut rng, cx)
        }
    });

    let font = test_font();
    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer.clone(),
            font,
            font_size,
            wrap_width,
            buffer_start_excerpt_header_height,
            excerpt_header_height,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });
    let mut notifications = observe(&map, cx);
    let mut fold_count = 0;
    let mut blocks = Vec::new();

    let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
    log::info!("buffer text: {:?}", snapshot.buffer_snapshot().text());
    log::info!("fold text: {:?}", snapshot.fold_snapshot().text());
    log::info!("tab text: {:?}", snapshot.tab_snapshot().text());
    log::info!("wrap text: {:?}", snapshot.wrap_snapshot().text());
    log::info!("block text: {:?}", snapshot.block_snapshot.text());
    log::info!("display text: {:?}", snapshot.text());

    for _i in 0..operations {
        match rng.random_range(0..100) {
            0..=19 => {
                wrap_width = if rng.random_bool(0.2) {
                    None
                } else {
                    Some(px(rng.random_range(0.0..=max_wrap_width)))
                };
                log::info!("setting wrap width to {:?}", wrap_width);
                map.update(cx, |map, cx| map.set_wrap_width(wrap_width, cx));
            }
            20..=29 => {
                let mut tab_sizes = vec![1, 2, 3, 4];
                tab_sizes.remove((tab_size - 1) as usize);
                tab_size = *tab_sizes.choose(&mut rng).unwrap();
                log::info!("setting tab size to {:?}", tab_size);
                cx.update(|cx| {
                    cx.update_global::<SettingsStore, _>(|store, cx| {
                        store.update_user_settings(cx, |s| {
                            s.project.all_languages.defaults.tab_size = NonZeroU32::new(tab_size);
                        });
                    });
                });
            }
            30..=44 => {
                map.update(cx, |map, cx| {
                    if rng.random() || blocks.is_empty() {
                        let snapshot = map.snapshot(cx);
                        let buffer = snapshot.buffer_snapshot();
                        let block_properties = (0..rng.random_range(1..=1))
                            .map(|_| {
                                let position = buffer.anchor_after(buffer.clip_offset(
                                    rng.random_range(MultiBufferOffset(0)..=buffer.len()),
                                    Bias::Left,
                                ));

                                let placement = if rng.random() {
                                    BlockPlacement::Above(position)
                                } else {
                                    BlockPlacement::Below(position)
                                };
                                let height = rng.random_range(1..5);
                                log::info!(
                                    "inserting block {:?} with height {}",
                                    placement.as_ref().map(|p| p.to_point(&buffer)),
                                    height
                                );
                                let priority = rng.random_range(1..100);
                                BlockProperties {
                                    placement,
                                    style: BlockStyle::Fixed,
                                    height: Some(height),
                                    render: Arc::new(|_| div().into_any()),
                                    priority,
                                }
                            })
                            .collect::<Vec<_>>();
                        blocks.extend(map.insert_blocks(block_properties, cx));
                    } else {
                        blocks.shuffle(&mut rng);
                        let remove_count = rng.random_range(1..=4.min(blocks.len()));
                        let block_ids_to_remove = (0..remove_count)
                            .map(|_| blocks.remove(rng.random_range(0..blocks.len())))
                            .collect();
                        log::info!("removing block ids {:?}", block_ids_to_remove);
                        map.remove_blocks(block_ids_to_remove, cx);
                    }
                });
            }
            45..=79 => {
                let mut ranges = Vec::new();
                for _ in 0..rng.random_range(1..=3) {
                    buffer.read_with(cx, |buffer, cx| {
                        let buffer = buffer.read(cx);
                        let end = buffer.clip_offset(
                            rng.random_range(MultiBufferOffset(0)..=buffer.len()),
                            Right,
                        );
                        let start =
                            buffer.clip_offset(rng.random_range(MultiBufferOffset(0)..=end), Left);
                        ranges.push(start..end);
                    });
                }

                if rng.random() && fold_count > 0 {
                    log::info!("unfolding ranges: {:?}", ranges);
                    map.update(cx, |map, cx| {
                        map.unfold_intersecting(ranges, true, cx);
                    });
                } else {
                    log::info!("folding ranges: {:?}", ranges);
                    map.update(cx, |map, cx| {
                        map.fold(
                            ranges
                                .into_iter()
                                .map(|range| Crease::simple(range, FoldPlaceholder::test()))
                                .collect(),
                            cx,
                        );
                    });
                }
            }
            _ => {
                buffer.update(cx, |buffer, cx| buffer.randomly_mutate(&mut rng, 5, cx));
            }
        }

        if map.read_with(cx, |map, cx| map.is_rewrapping(cx)) {
            notifications.next().await.unwrap();
        }

        let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
        fold_count = snapshot.fold_count();
        log::info!("buffer text: {:?}", snapshot.buffer_snapshot().text());
        log::info!("fold text: {:?}", snapshot.fold_snapshot().text());
        log::info!("tab text: {:?}", snapshot.tab_snapshot().text());
        log::info!("wrap text: {:?}", snapshot.wrap_snapshot().text());
        log::info!("block text: {:?}", snapshot.block_snapshot.text());
        log::info!("display text: {:?}", snapshot.text());

        // Line boundaries
        let buffer = snapshot.buffer_snapshot();
        for _ in 0..5 {
            let row = rng.random_range(0..=buffer.max_point().row);
            let column = rng.random_range(0..=buffer.line_len(MultiBufferRow(row)));
            let point = buffer.clip_point(Point::new(row, column), Left);

            let (prev_buffer_bound, prev_display_bound) = snapshot.prev_line_boundary(point);
            let (next_buffer_bound, next_display_bound) = snapshot.next_line_boundary(point);

            assert!(prev_buffer_bound <= point);
            assert!(next_buffer_bound >= point);
            assert_eq!(prev_buffer_bound.column, 0);
            assert_eq!(prev_display_bound.column(), 0);
            if next_buffer_bound < buffer.max_point() {
                assert_eq!(buffer.chars_at(next_buffer_bound).next(), Some('\n'));
            }

            assert_eq!(
                prev_display_bound,
                prev_buffer_bound.to_display_point(&snapshot),
                "row boundary before {:?}. reported buffer row boundary: {:?}",
                point,
                prev_buffer_bound
            );
            assert_eq!(
                next_display_bound,
                next_buffer_bound.to_display_point(&snapshot),
                "display row boundary after {:?}. reported buffer row boundary: {:?}",
                point,
                next_buffer_bound
            );
            assert_eq!(
                prev_buffer_bound,
                prev_display_bound.to_point(&snapshot),
                "row boundary before {:?}. reported display row boundary: {:?}",
                point,
                prev_display_bound
            );
            assert_eq!(
                next_buffer_bound,
                next_display_bound.to_point(&snapshot),
                "row boundary after {:?}. reported display row boundary: {:?}",
                point,
                next_display_bound
            );
        }

        // Movement
        let min_point = snapshot.clip_point(DisplayPoint::new(DisplayRow(0), 0), Left);
        let max_point = snapshot.clip_point(snapshot.max_point(), Right);
        for _ in 0..5 {
            let row = rng.random_range(0..=snapshot.max_point().row().0);
            let column = rng.random_range(0..=snapshot.line_len(DisplayRow(row)));
            let point = snapshot.clip_point(DisplayPoint::new(DisplayRow(row), column), Left);

            log::info!("Moving from point {:?}", point);

            let moved_right = movement::right(&snapshot, point);
            log::info!("Right {:?}", moved_right);
            if point < max_point {
                assert!(moved_right > point);
                if point.column() == snapshot.line_len(point.row())
                    || snapshot.soft_wrap_indent(point.row()).is_some()
                        && point.column() == snapshot.line_len(point.row()) - 1
                {
                    assert!(moved_right.row() > point.row());
                }
            } else {
                assert_eq!(moved_right, point);
            }

            let moved_left = movement::left(&snapshot, point);
            log::info!("Left {:?}", moved_left);
            if point > min_point {
                assert!(moved_left < point);
                if point.column() == 0 {
                    assert!(moved_left.row() < point.row());
                }
            } else {
                assert_eq!(moved_left, point);
            }
        }
    }
}

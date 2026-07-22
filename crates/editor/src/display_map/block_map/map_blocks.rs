use super::*;

impl BlockMap {
    #[ztracing::instrument(skip_all)]
    pub fn replace_blocks(&mut self, mut renderers: HashMap<CustomBlockId, RenderBlock>) {
        for block in &mut self.custom_blocks {
            if let Some(render) = renderers.remove(&block.id) {
                *block.render.lock() = render;
            }
        }
    }

    /// Guarantees that `wrap_row_for` is called with points in increasing order.
    #[ztracing::instrument(skip_all)]
    fn header_and_footer_blocks<'a, R, T>(
        &'a self,
        buffer: &'a multi_buffer::MultiBufferSnapshot,
        range: R,
        mut wrap_row_for: impl 'a + FnMut(Point, Bias) -> WrapRow,
    ) -> impl Iterator<Item = (BlockPlacement<WrapRow>, Block)> + 'a
    where
        R: RangeBounds<T>,
        T: multi_buffer::ToOffset,
    {
        let mut boundaries = buffer.excerpt_boundaries_in_range(range).peekable();

        std::iter::from_fn(move || {
            loop {
                let excerpt_boundary = boundaries.next()?;
                let wrap_row = wrap_row_for(Point::new(excerpt_boundary.row.0, 0), Bias::Left);

                let new_buffer_id = match (&excerpt_boundary.prev, &excerpt_boundary.next) {
                    (None, next) => Some(next.buffer_id()),
                    (Some(prev), next) => {
                        if prev.buffer_id() != next.buffer_id() {
                            Some(next.buffer_id())
                        } else {
                            None
                        }
                    }
                };

                let mut height = 0;

                if let Some(new_buffer_id) = new_buffer_id {
                    let first_excerpt = excerpt_boundary.next.clone();
                    if self.buffers_with_disabled_headers.contains(&new_buffer_id) {
                        continue;
                    }
                    if self.folded_buffers.contains(&new_buffer_id) && buffer.show_headers() {
                        let mut last_excerpt_end_row = first_excerpt.end_row;

                        while let Some(next_boundary) = boundaries.peek() {
                            if next_boundary.next.buffer_id() == new_buffer_id {
                                last_excerpt_end_row = next_boundary.next.end_row;
                            } else {
                                break;
                            }

                            boundaries.next();
                        }
                        let wrap_end_row = wrap_row_for(
                            Point::new(
                                last_excerpt_end_row.0,
                                buffer.line_len(last_excerpt_end_row),
                            ),
                            Bias::Right,
                        );

                        return Some((
                            BlockPlacement::Replace(wrap_row..=wrap_end_row),
                            Block::FoldedBuffer {
                                height: height + self.buffer_header_height,
                                first_excerpt,
                            },
                        ));
                    }
                }

                let starts_new_buffer = new_buffer_id.is_some();
                let block = if starts_new_buffer && buffer.show_headers() {
                    height += self.buffer_header_height;
                    Block::BufferHeader {
                        excerpt: excerpt_boundary.next,
                        height,
                    }
                } else if excerpt_boundary.prev.is_some() {
                    height += self.excerpt_header_height;
                    Block::ExcerptBoundary {
                        excerpt: excerpt_boundary.next,
                        height,
                    }
                } else {
                    continue;
                };

                return Some((BlockPlacement::Above(wrap_row), block));
            }
        })
    }

    fn spacer_blocks(
        &self,
        bounds: (Bound<MultiBufferPoint>, Bound<MultiBufferPoint>),
        wrap_snapshot: &WrapSnapshot,
        companion_snapshot: &WrapSnapshot,
        companion: &Companion,
        display_map_id: EntityId,
    ) -> Vec<(BlockPlacement<WrapRow>, Block)> {
        let our_buffer = wrap_snapshot.buffer_snapshot();
        let companion_buffer = companion_snapshot.buffer_snapshot();

        let range = match bounds {
            (Bound::Included(start), Bound::Excluded(end)) => start..end,
            (Bound::Included(start), Bound::Unbounded) => start..wrap_snapshot.buffer().max_point(),
            _ => unreachable!(),
        };
        let mut patches = companion.convert_rows_to_companion(
            display_map_id,
            companion_buffer,
            our_buffer,
            range,
        );
        if let Some(patch) = patches.last()
            && let Bound::Excluded(end) = bounds.1
            && end == wrap_snapshot.buffer().max_point()
            && patch.source_excerpt_range.is_empty()
        {
            patches.pop();
        }

        let mut our_inlay_point_cursor = wrap_snapshot.inlay_point_cursor();
        let mut our_fold_point_cursor = wrap_snapshot.fold_point_cursor();
        let mut our_tab_point_cursor = wrap_snapshot.tab_point_cursor();
        let mut our_wrap_point_cursor = wrap_snapshot.wrap_point_cursor();

        let mut our_wrapper = |our_point: Point, bias: Bias| {
            our_wrap_point_cursor
                .map(our_tab_point_cursor.map(
                    our_fold_point_cursor.map(our_inlay_point_cursor.map(our_point, bias), bias),
                ))
                .row()
        };
        let mut companion_wrapper = |their_point: Point, bias: Bias| {
            // TODO(split-diff) fix companion points being passed in decreasing order
            let inlay_point = companion_snapshot
                .inlay_snapshot
                .inlay_point_cursor()
                .map(their_point, bias);
            let fold_point = companion_snapshot.to_fold_point(inlay_point, bias);
            let tab_point = companion_snapshot.fold_point_to_tab_point(fold_point);
            companion_snapshot.tab_point_to_wrap_point(tab_point).row()
        };
        fn determine_spacer(
            our_wrapper: &mut dyn FnMut(Point, Bias) -> WrapRow,
            companion_wrapper: &mut dyn FnMut(Point, Bias) -> WrapRow,
            our_point: Point,
            their_point: Point,
            delta: i32,
            bias: Bias,
        ) -> (i32, Option<(WrapRow, u32)>) {
            let our_wrap = our_wrapper(our_point, bias);
            let companion_wrap = companion_wrapper(their_point, bias);
            let new_delta = companion_wrap.0 as i32 - our_wrap.0 as i32;

            let spacer = if new_delta > delta {
                let height = (new_delta - delta) as u32;
                Some((our_wrap, height))
            } else {
                None
            };
            (new_delta, spacer)
        }

        let mut result = Vec::new();

        for excerpt in patches {
            let mut source_points = (excerpt.edited_range.start.row..=excerpt.edited_range.end.row)
                .map(|row| MultiBufferPoint::new(row, 0))
                .chain(if excerpt.edited_range.end.column > 0 {
                    Some(excerpt.edited_range.end)
                } else {
                    None
                })
                .peekable();
            let last_source_point = if excerpt.edited_range.end.column > 0 {
                excerpt.edited_range.end
            } else {
                MultiBufferPoint::new(excerpt.edited_range.end.row, 0)
            };

            let Some(first_point) = source_points.peek().copied() else {
                continue;
            };
            let edit_for_first_point = excerpt.patch.edit_for_old_position(first_point);

            // Because we calculate spacers based on differences in wrap row
            // counts between the RHS and LHS for corresponding buffer points,
            // we need to calibrate our expectations based on the difference
            // in counts before the start of the edit. This difference in
            // counts should have been balanced already by spacers above this
            // edit, so we only need to insert spacers for when the difference
            // in counts diverges from that baseline value.
            let (our_baseline, their_baseline) = if edit_for_first_point.old.start < first_point {
                // Case 1: We are inside a hunk/group--take the start of the hunk/group on both sides as the baseline.
                (
                    edit_for_first_point.old.start,
                    edit_for_first_point.new.start,
                )
            } else if first_point.row > excerpt.source_excerpt_range.start.row {
                // Case 2: We are not inside a hunk/group--go back by one row to find the baseline.
                let prev_point = Point::new(first_point.row - 1, 0);
                let edit_for_prev_point = excerpt.patch.edit_for_old_position(prev_point);
                (prev_point, edit_for_prev_point.new.end)
            } else {
                // Case 3: We are at the start of the excerpt--no previous row to use as the baseline.
                (first_point, edit_for_first_point.new.start)
            };
            let our_baseline = our_wrapper(our_baseline, Bias::Left);
            let their_baseline = companion_wrapper(
                their_baseline.min(excerpt.target_excerpt_range.end),
                Bias::Left,
            );

            let mut delta = their_baseline.0 as i32 - our_baseline.0 as i32;

            while let Some(source_point) = source_points.next() {
                let mut current_boundary = source_point;
                let current_edit = excerpt.patch.edit_for_old_position(current_boundary);
                let current_range = current_edit.new;

                if current_boundary.column > 0 {
                    debug_assert_eq!(current_boundary, excerpt.source_excerpt_range.end);
                    break;
                }

                if current_edit.old.start < current_boundary {
                    while let Some(next_point) = source_points.peek().copied() {
                        let edit_for_next_point = excerpt.patch.edit_for_old_position(next_point);
                        if edit_for_next_point.new.end > current_range.end {
                            break;
                        }
                        current_boundary = next_point;
                        source_points.next();
                    }

                    let (new_delta, spacer) = determine_spacer(
                        &mut our_wrapper,
                        &mut companion_wrapper,
                        current_boundary,
                        current_range.end.min(excerpt.target_excerpt_range.end),
                        delta,
                        Bias::Left,
                    );

                    delta = new_delta;
                    if let Some((wrap_row, height)) = spacer {
                        result.push((
                            BlockPlacement::Above(wrap_row),
                            Block::Spacer {
                                id: SpacerId(self.next_block_id.fetch_add(1, SeqCst)),
                                height,
                                is_below: false,
                            },
                        ));
                    }
                    continue;
                }

                let (delta_at_start, mut spacer_at_start) = determine_spacer(
                    &mut our_wrapper,
                    &mut companion_wrapper,
                    current_boundary,
                    current_range.start.min(excerpt.target_excerpt_range.end),
                    delta,
                    Bias::Left,
                );
                delta = delta_at_start;

                while let Some(next_point) = source_points.peek().copied() {
                    let edit_for_next_point = excerpt.patch.edit_for_old_position(next_point);
                    if edit_for_next_point.new.end > current_range.end {
                        break;
                    }

                    if let Some((wrap_row, height)) = spacer_at_start.take() {
                        result.push((
                            BlockPlacement::Above(wrap_row),
                            Block::Spacer {
                                id: SpacerId(self.next_block_id.fetch_add(1, SeqCst)),
                                height,
                                is_below: false,
                            },
                        ));
                    }

                    current_boundary = next_point;
                    source_points.next();
                }

                if current_boundary.column > 0 {
                    debug_assert_eq!(current_boundary, excerpt.source_excerpt_range.end);
                    break;
                }

                let edit_for_current_boundary =
                    excerpt.patch.edit_for_old_position(current_boundary);

                let spacer_at_end = if current_boundary == edit_for_current_boundary.old.end {
                    let (delta_at_end, spacer_at_end) = determine_spacer(
                        &mut our_wrapper,
                        &mut companion_wrapper,
                        current_boundary,
                        current_range.end.min(excerpt.target_excerpt_range.end),
                        delta,
                        Bias::Left,
                    );
                    delta = delta_at_end;
                    spacer_at_end
                } else {
                    None
                };

                if let Some((wrap_row, mut height)) = spacer_at_start {
                    if let Some((_, additional_height)) = spacer_at_end {
                        height += additional_height;
                    }
                    result.push((
                        BlockPlacement::Above(wrap_row),
                        Block::Spacer {
                            id: SpacerId(self.next_block_id.fetch_add(1, SeqCst)),
                            height,
                            is_below: false,
                        },
                    ));
                } else if let Some((wrap_row, height)) = spacer_at_end {
                    result.push((
                        BlockPlacement::Above(wrap_row),
                        Block::Spacer {
                            id: SpacerId(self.next_block_id.fetch_add(1, SeqCst)),
                            height,
                            is_below: false,
                        },
                    ));
                }
            }

            if last_source_point == excerpt.source_excerpt_range.end {
                let (_new_delta, spacer) = determine_spacer(
                    &mut our_wrapper,
                    &mut companion_wrapper,
                    last_source_point,
                    excerpt.target_excerpt_range.end,
                    delta,
                    Bias::Right,
                );
                if let Some((wrap_row, height)) = spacer {
                    result.push((
                        BlockPlacement::Below(wrap_row),
                        Block::Spacer {
                            id: SpacerId(self.next_block_id.fetch_add(1, SeqCst)),
                            height,
                            is_below: true,
                        },
                    ));
                }
            }
        }

        result
    }

    #[ztracing::instrument(skip_all)]
    fn sort_blocks(blocks: &mut Vec<(BlockPlacement<WrapRow>, Block)>) {
        blocks.sort_unstable_by(|(placement_a, block_a), (placement_b, block_b)| {
            placement_a
                .start()
                .cmp(placement_b.start())
                .then_with(|| placement_b.end().cmp(placement_a.end()))
                .then_with(|| placement_a.tie_break().cmp(&placement_b.tie_break()))
                .then_with(|| {
                    if block_a.is_header() {
                        Ordering::Less
                    } else if block_b.is_header() {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                })
                .then_with(|| match (block_a, block_b) {
                    (
                        Block::ExcerptBoundary {
                            excerpt: excerpt_a, ..
                        }
                        | Block::BufferHeader {
                            excerpt: excerpt_a, ..
                        },
                        Block::ExcerptBoundary {
                            excerpt: excerpt_b, ..
                        }
                        | Block::BufferHeader {
                            excerpt: excerpt_b, ..
                        },
                    ) => Some(excerpt_a.start_text_anchor().opaque_id())
                        .cmp(&Some(excerpt_b.start_text_anchor().opaque_id())),
                    (
                        Block::ExcerptBoundary { .. } | Block::BufferHeader { .. },
                        Block::Spacer { .. } | Block::Custom(_),
                    ) => Ordering::Less,
                    (
                        Block::Spacer { .. } | Block::Custom(_),
                        Block::ExcerptBoundary { .. } | Block::BufferHeader { .. },
                    ) => Ordering::Greater,
                    (Block::Spacer { .. }, Block::Custom(_)) => Ordering::Less,
                    (Block::Custom(_), Block::Spacer { .. }) => Ordering::Greater,
                    (Block::Custom(block_a), Block::Custom(block_b)) => block_a
                        .priority
                        .cmp(&block_b.priority)
                        .then_with(|| block_a.id.cmp(&block_b.id)),
                    _ => {
                        unreachable!("comparing blocks: {block_a:?} vs {block_b:?}")
                    }
                })
        });
        blocks.dedup_by(|right, left| match (left.0.clone(), right.0.clone()) {
            (BlockPlacement::Replace(range), BlockPlacement::Above(row))
            | (BlockPlacement::Replace(range), BlockPlacement::Below(row)) => range.contains(&row),
            (BlockPlacement::Replace(range_a), BlockPlacement::Replace(range_b)) => {
                if range_a.end() >= range_b.start() && range_a.start() <= range_b.end() {
                    left.0 = BlockPlacement::Replace(
                        *range_a.start()..=*range_a.end().max(range_b.end()),
                    );
                    true
                } else {
                    false
                }
            }
            _ => false,
        });
    }
}

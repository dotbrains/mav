use super::*;

fn method_motion(
    map: &DisplaySnapshot,
    mut display_point: DisplayPoint,
    times: usize,
    direction: Direction,
    is_start: bool,
) -> DisplayPoint {
    let snapshot = map.buffer_snapshot();
    if snapshot.as_singleton().is_none() {
        return display_point;
    }

    for _ in 0..times {
        let offset = map
            .display_point_to_point(display_point, Bias::Left)
            .to_offset(&snapshot);
        let range = if direction == Direction::Prev {
            MultiBufferOffset(0)..offset
        } else {
            offset..snapshot.len()
        };

        let possibilities = snapshot
            .text_object_ranges(range, language::TreeSitterOptions::max_start_depth(4))
            .filter_map(|(range, object)| {
                if !matches!(object, language::TextObject::AroundFunction) {
                    return None;
                }

                let relevant = if is_start { range.start } else { range.end };
                if direction == Direction::Prev && relevant < offset {
                    Some(relevant)
                } else if direction == Direction::Next && relevant > offset + 1usize {
                    Some(relevant)
                } else {
                    None
                }
            });

        let dest = if direction == Direction::Prev {
            possibilities.max().unwrap_or(offset)
        } else {
            possibilities.min().unwrap_or(offset)
        };
        let new_point = map.clip_point(dest.to_display_point(map), Bias::Left);
        if new_point == display_point {
            break;
        }
        display_point = new_point;
    }
    display_point
}

fn comment_motion(
    map: &DisplaySnapshot,
    mut display_point: DisplayPoint,
    times: usize,
    direction: Direction,
) -> DisplayPoint {
    let snapshot = map.buffer_snapshot();
    if snapshot.as_singleton().is_none() {
        return display_point;
    }

    for _ in 0..times {
        let offset = map
            .display_point_to_point(display_point, Bias::Left)
            .to_offset(&snapshot);
        let range = if direction == Direction::Prev {
            MultiBufferOffset(0)..offset
        } else {
            offset..snapshot.len()
        };

        let possibilities = snapshot
            .text_object_ranges(range, language::TreeSitterOptions::max_start_depth(6))
            .filter_map(|(range, object)| {
                if !matches!(object, language::TextObject::AroundComment) {
                    return None;
                }

                let relevant = if direction == Direction::Prev {
                    range.start
                } else {
                    range.end
                };
                if direction == Direction::Prev && relevant < offset {
                    Some(relevant)
                } else if direction == Direction::Next && relevant > offset + 1usize {
                    Some(relevant)
                } else {
                    None
                }
            });

        let dest = if direction == Direction::Prev {
            possibilities.max().unwrap_or(offset)
        } else {
            possibilities.min().unwrap_or(offset)
        };
        let new_point = map.clip_point(dest.to_display_point(map), Bias::Left);
        if new_point == display_point {
            break;
        }
        display_point = new_point;
    }

    display_point
}

fn section_motion(
    map: &DisplaySnapshot,
    mut display_point: DisplayPoint,
    times: usize,
    direction: Direction,
    is_start: bool,
) -> DisplayPoint {
    if map.buffer_snapshot().as_singleton().is_some() {
        for _ in 0..times {
            let offset = map
                .display_point_to_point(display_point, Bias::Left)
                .to_offset(&map.buffer_snapshot());
            let range = if direction == Direction::Prev {
                MultiBufferOffset(0)..offset
            } else {
                offset..map.buffer_snapshot().len()
            };

            // we set a max start depth here because we want a section to only be "top level"
            // similar to vim's default of '{' in the first column.
            // (and without it, ]] at the start of editor.rs is -very- slow)
            let mut possibilities = map
                .buffer_snapshot()
                .text_object_ranges(range, language::TreeSitterOptions::max_start_depth(3))
                .filter(|(_, object)| {
                    matches!(
                        object,
                        language::TextObject::AroundClass | language::TextObject::AroundFunction
                    )
                })
                .collect::<Vec<_>>();
            possibilities.sort_by_key(|(range_a, _)| range_a.start);
            let mut prev_end = None;
            let possibilities = possibilities.into_iter().filter_map(|(range, t)| {
                if t == language::TextObject::AroundFunction
                    && prev_end.is_some_and(|prev_end| prev_end > range.start)
                {
                    return None;
                }
                prev_end = Some(range.end);

                let relevant = if is_start { range.start } else { range.end };
                if direction == Direction::Prev && relevant < offset {
                    Some(relevant)
                } else if direction == Direction::Next && relevant > offset + 1usize {
                    Some(relevant)
                } else {
                    None
                }
            });

            let offset = if direction == Direction::Prev {
                possibilities.max().unwrap_or(MultiBufferOffset(0))
            } else {
                possibilities.min().unwrap_or(map.buffer_snapshot().len())
            };

            let new_point = map.clip_point(offset.to_display_point(map), Bias::Left);
            if new_point == display_point {
                break;
            }
            display_point = new_point;
        }
        return display_point;
    };

    for _ in 0..times {
        let next_point = if is_start {
            movement::start_of_excerpt(map, display_point, direction)
        } else {
            movement::end_of_excerpt(map, display_point, direction)
        };
        if next_point == display_point {
            break;
        }
        display_point = next_point;
    }

    display_point
}

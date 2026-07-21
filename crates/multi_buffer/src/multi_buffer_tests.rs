use super::*;
use buffer_diff::{DiffHunkStatus, DiffHunkStatusKind};
use gpui::{App, Entity, TestAppContext};
use indoc::indoc;
use language::{Buffer, Rope};
use parking_lot::RwLock;
use rand::prelude::*;
use settings::SettingsStore;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use util::RandomCharIter;
use util::rel_path::rel_path;
use util::test::sample_text;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

mod anchor_boundary_tests;
mod basic_diff_tests;
mod chunk_bitmap_tests;
mod diff_hunk_tests;
mod empty_anchor_tests;
mod excerpt_lifecycle_tests;
mod excerpt_range_tests;
mod inverted_diff_tests;
mod map_excerpt_tests;
mod path_replacement_tests;
mod range_mapping_tests;
mod singleton_tests;
mod tail_behavior_tests;
mod title_tests;
mod word_diff_tests;

#[gpui::test]
async fn test_diff_hunks_with_multiple_excerpts(cx: &mut TestAppContext) {
    let base_text_1 = indoc!(
        "
        one
        two
            three
        four
        five
        six
        "
    );
    let text_1 = indoc!(
        "
        ZERO
        one
        TWO
            three
        six
        "
    );
    let base_text_2 = indoc!(
        "
        seven
          eight
        nine
        ten
        eleven
        twelve
        "
    );
    let text_2 = indoc!(
        "
          eight
        nine
        eleven
        THIRTEEN
        FOURTEEN
        "
    );

    let buffer_1 = cx.new(|cx| Buffer::local(text_1, cx));
    let buffer_2 = cx.new(|cx| Buffer::local(text_2, cx));
    let diff_1 = cx.new(|cx| {
        BufferDiff::new_with_base_text(base_text_1, &buffer_1.read(cx).text_snapshot(), cx)
    });
    let diff_2 = cx.new(|cx| {
        BufferDiff::new_with_base_text(base_text_2, &buffer_2.read(cx).text_snapshot(), cx)
    });
    cx.run_until_parked();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::zero()..buffer_1.read(cx).max_point()],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::zero()..buffer_2.read(cx).max_point()],
            0,
            cx,
        );
        multibuffer.add_diff(diff_1.clone(), cx);
        multibuffer.add_diff(diff_2.clone(), cx);
        multibuffer
    });

    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });
    assert_eq!(
        snapshot.text(),
        indoc!(
            "
            ZERO
            one
            TWO
                three
            six

              eight
            nine
            eleven
            THIRTEEN
            FOURTEEN
            "
        ),
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
            + ZERO
              one
            - two
            + TWO
                  three
            - four
            - five
              six

            - seven
                eight
              nine
            - ten
              eleven
            - twelve
            + THIRTEEN
            + FOURTEEN
            "
        ),
    );

    let id_1 = buffer_1.read_with(cx, |buffer, _| buffer.remote_id());
    let id_2 = buffer_2.read_with(cx, |buffer, _| buffer.remote_id());
    let base_id_1 = diff_1.read_with(cx, |diff, cx| diff.base_text(cx).remote_id());
    let base_id_2 = diff_2.read_with(cx, |diff, cx| diff.base_text(cx).remote_id());

    let buffer_lines = (0..=snapshot.max_row().0)
        .map(|row| {
            let (buffer, range) = snapshot.buffer_line_for_row(MultiBufferRow(row))?;
            Some((
                buffer.remote_id(),
                buffer.text_for_range(range).collect::<String>(),
            ))
        })
        .collect::<Vec<_>>();
    pretty_assertions::assert_eq!(
        buffer_lines,
        [
            Some((id_1, "ZERO".into())),
            Some((id_1, "one".into())),
            Some((base_id_1, "two".into())),
            Some((id_1, "TWO".into())),
            Some((id_1, "    three".into())),
            Some((base_id_1, "four".into())),
            Some((base_id_1, "five".into())),
            Some((id_1, "six".into())),
            Some((id_1, "".into())),
            Some((base_id_2, "seven".into())),
            Some((id_2, "  eight".into())),
            Some((id_2, "nine".into())),
            Some((base_id_2, "ten".into())),
            Some((id_2, "eleven".into())),
            Some((base_id_2, "twelve".into())),
            Some((id_2, "THIRTEEN".into())),
            Some((id_2, "FOURTEEN".into())),
            Some((id_2, "".into())),
        ]
    );

    let buffer_ids_by_range = [
        (Point::new(0, 0)..Point::new(0, 0), &[id_1] as &[_]),
        (Point::new(0, 0)..Point::new(2, 0), &[id_1]),
        (Point::new(2, 0)..Point::new(2, 0), &[id_1]),
        (Point::new(3, 0)..Point::new(3, 0), &[id_1]),
        (Point::new(8, 0)..Point::new(9, 0), &[id_1]),
        (Point::new(8, 0)..Point::new(10, 0), &[id_1, id_2]),
        (Point::new(9, 0)..Point::new(9, 0), &[id_2]),
    ];
    for (range, buffer_ids) in buffer_ids_by_range {
        assert_eq!(
            snapshot
                .buffer_ids_for_range(range.clone())
                .collect::<Vec<_>>(),
            buffer_ids,
            "buffer_ids_for_range({range:?}"
        );
    }

    assert_position_translation(&snapshot);
    assert_line_indents(&snapshot);

    assert_eq!(
        snapshot
            .diff_hunks_in_range(MultiBufferOffset(0)..snapshot.len())
            .map(|hunk| hunk.row_range.start.0..hunk.row_range.end.0)
            .collect::<Vec<_>>(),
        &[0..1, 2..4, 5..7, 9..10, 12..13, 14..17]
    );

    buffer_2.update(cx, |buffer, cx| {
        buffer.edit_via_marked_text(
            indoc!(
                "
                  eight
                «»eleven
                THIRTEEN
                FOURTEEN
                "
            ),
            None,
            cx,
        );
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
            + ZERO
              one
            - two
            + TWO
                  three
            - four
            - five
              six

            - seven
                eight
              eleven
            - twelve
            + THIRTEEN
            + FOURTEEN
            "
        ),
    );

    assert_line_indents(&snapshot);
}

/// A naive implementation of a multi-buffer that does not maintain
/// any derived state, used for comparison in a randomized test.
#[derive(Default)]
struct ReferenceMultibuffer {
    excerpts: Vec<ReferenceExcerpt>,
    diffs: HashMap<BufferId, Entity<BufferDiff>>,
    inverted_diffs: HashMap<BufferId, (Entity<BufferDiff>, Entity<language::Buffer>)>,
    expanded_diff_hunks_by_buffer: HashMap<BufferId, Vec<text::Anchor>>,
}

#[derive(Clone, Debug)]
struct ReferenceExcerpt {
    path_key: PathKey,
    path_key_index: PathKeyIndex,
    buffer: Entity<Buffer>,
    range: Range<text::Anchor>,
}

#[derive(Clone, Debug)]
struct ReferenceRegion {
    buffer_id: Option<BufferId>,
    range: Range<usize>,
    buffer_range: Range<Point>,
    // if this is a deleted hunk, the main buffer anchor to which the deleted content is attached
    deleted_hunk_anchor: Option<text::Anchor>,
    status: Option<DiffHunkStatus>,
    excerpt: Option<ReferenceExcerpt>,
}

impl ReferenceMultibuffer {
    fn expand_excerpts(
        &mut self,
        excerpts: &HashSet<ExcerptRange<text::Anchor>>,
        line_count: u32,
        cx: &mut App,
    ) {
        use text::AnchorRangeExt as _;

        if line_count == 0 || excerpts.is_empty() {
            return;
        }

        let mut excerpts_by_buffer: HashMap<BufferId, Vec<ExcerptRange<text::Anchor>>> =
            HashMap::default();
        for excerpt in excerpts {
            excerpts_by_buffer
                .entry(excerpt.context.start.buffer_id)
                .or_default()
                .push(excerpt.clone())
        }

        for (buffer_id, excerpts_to_expand) in excerpts_by_buffer {
            let mut buffer = None;
            let mut buffer_snapshot = None;
            let mut path = None;
            let mut path_key_index = None;
            let mut new_ranges =
                self.excerpts
                    .iter()
                    .filter(|excerpt| excerpt.range.start.buffer_id == buffer_id)
                    .map(|excerpt| {
                        let snapshot = excerpt.buffer.read(cx).snapshot();
                        let mut range = excerpt.range.to_point(&snapshot);
                        if excerpts_to_expand.iter().any(|info| {
                            excerpt.range.contains_anchor(info.context.start, &snapshot)
                        }) {
                            range.start = Point::new(range.start.row.saturating_sub(line_count), 0);
                            range.end = snapshot
                                .clip_point(Point::new(range.end.row + line_count, 0), Bias::Left);
                            range.end.column = snapshot.line_len(range.end.row);
                        }
                        buffer = Some(excerpt.buffer.clone());
                        buffer_snapshot = Some(snapshot);
                        path = Some(excerpt.path_key.clone());
                        path_key_index = Some(excerpt.path_key_index);
                        ExcerptRange::new(range)
                    })
                    .collect::<Vec<_>>();

            new_ranges.sort_by_key(|nr| nr.context.start);

            self.set_excerpts(
                path.unwrap(),
                path_key_index.unwrap(),
                buffer.unwrap(),
                &buffer_snapshot.unwrap(),
                new_ranges,
                cx,
            );
        }
    }

    fn set_excerpts(
        &mut self,
        path_key: PathKey,
        path_key_index: PathKeyIndex,
        buffer: Entity<Buffer>,
        buffer_snapshot: &BufferSnapshot,
        ranges: Vec<ExcerptRange<Point>>,
        cx: &mut App,
    ) {
        self.excerpts.retain(|excerpt| {
            excerpt.path_key != path_key && excerpt.buffer.entity_id() != buffer.entity_id()
        });

        let ranges = MultiBuffer::merge_excerpt_ranges(&ranges);

        let (Ok(ix) | Err(ix)) = self
            .excerpts
            .binary_search_by(|probe| probe.path_key.cmp(&path_key));
        self.excerpts.splice(
            ix..ix,
            ranges.into_iter().map(|range| ReferenceExcerpt {
                path_key: path_key.clone(),
                path_key_index,
                buffer: buffer.clone(),
                range: buffer_snapshot.anchor_before(range.context.start)
                    ..buffer_snapshot.anchor_after(range.context.end),
            }),
        );
        self.update_expanded_diff_hunks_for_buffer(buffer_snapshot.remote_id(), cx);
    }

    fn expand_diff_hunks(&mut self, path_key: PathKey, range: Range<text::Anchor>, cx: &App) {
        let excerpt = self
            .excerpts
            .iter_mut()
            .find(|e| {
                e.path_key == path_key
                    && e.range
                        .start
                        .cmp(&range.start, &e.buffer.read(cx).snapshot())
                        .is_le()
                    && e.range
                        .end
                        .cmp(&range.end, &e.buffer.read(cx).snapshot())
                        .is_ge()
            })
            .unwrap();
        let buffer = excerpt.buffer.read(cx).snapshot();
        let buffer_id = buffer.remote_id();

        // Skip inverted excerpts - hunks are always expanded
        if self.inverted_diffs.contains_key(&buffer_id) {
            return;
        }

        let Some(diff) = self.diffs.get(&buffer_id) else {
            return;
        };
        let excerpt_range = excerpt.range.to_point(&buffer);
        let expanded_diff_hunks = self
            .expanded_diff_hunks_by_buffer
            .entry(buffer_id)
            .or_default();
        for hunk in diff
            .read(cx)
            .snapshot(cx)
            .hunks_intersecting_range(range, &buffer)
        {
            let hunk_range = hunk.buffer_range.to_point(&buffer);
            if hunk_range.start < excerpt_range.start || hunk_range.start > excerpt_range.end {
                continue;
            }
            if let Err(ix) = expanded_diff_hunks
                .binary_search_by(|anchor| anchor.cmp(&hunk.buffer_range.start, &buffer))
            {
                log::info!(
                    "expanding diff hunk {:?}. excerpt range: {:?}, buffer {:?}",
                    hunk_range,
                    excerpt_range,
                    buffer.remote_id()
                );
                expanded_diff_hunks.insert(ix, hunk.buffer_range.start);
            } else {
                log::trace!("hunk {hunk_range:?} already expanded in excerpt");
            }
        }
    }

    fn expected_content(
        &self,
        cx: &App,
    ) -> (
        String,
        Vec<RowInfo>,
        HashSet<MultiBufferRow>,
        Vec<ReferenceRegion>,
    ) {
        use util::maybe;

        let mut text = String::new();
        let mut regions = Vec::<ReferenceRegion>::new();
        let mut excerpt_boundary_rows = HashSet::default();
        for excerpt in &self.excerpts {
            excerpt_boundary_rows.insert(MultiBufferRow(text.matches('\n').count() as u32));
            let buffer = excerpt.buffer.read(cx);
            let buffer_id = buffer.remote_id();
            let buffer_range = excerpt.range.to_offset(buffer);

            if let Some((diff, main_buffer)) = self.inverted_diffs.get(&buffer_id) {
                let diff_snapshot = diff.read(cx).snapshot(cx);
                let main_buffer_snapshot = main_buffer.read(cx).snapshot();

                let mut offset = buffer_range.start;
                for hunk in diff_snapshot.hunks_intersecting_base_text_range(
                    buffer_range.clone(),
                    &main_buffer_snapshot.text,
                ) {
                    let mut hunk_base_range = hunk.diff_base_byte_range.clone();

                    hunk_base_range.end = hunk_base_range.end.min(buffer_range.end);
                    if hunk_base_range.start > buffer_range.end
                        || hunk_base_range.start < buffer_range.start
                    {
                        continue;
                    }

                    // Add the text before the hunk
                    if hunk_base_range.start >= offset {
                        let len = text.len();
                        text.extend(buffer.text_for_range(offset..hunk_base_range.start));
                        if text.len() > len {
                            regions.push(ReferenceRegion {
                                buffer_id: Some(buffer_id),
                                range: len..text.len(),
                                buffer_range: (offset..hunk_base_range.start).to_point(&buffer),
                                status: None,
                                excerpt: Some(excerpt.clone()),
                                deleted_hunk_anchor: None,
                            });
                        }
                    }

                    // Add the "deleted" region (base text that's not in main)
                    if !hunk_base_range.is_empty() {
                        let len = text.len();
                        text.extend(buffer.text_for_range(hunk_base_range.clone()));
                        regions.push(ReferenceRegion {
                            buffer_id: Some(buffer_id),
                            range: len..text.len(),
                            buffer_range: hunk_base_range.to_point(&buffer),
                            status: Some(DiffHunkStatus::deleted(hunk.secondary_status)),
                            excerpt: Some(excerpt.clone()),
                            deleted_hunk_anchor: None,
                        });
                    }

                    offset = hunk_base_range.end;
                }

                // Add remaining buffer text
                let len = text.len();
                text.extend(buffer.text_for_range(offset..buffer_range.end));
                text.push('\n');
                regions.push(ReferenceRegion {
                    buffer_id: Some(buffer_id),
                    range: len..text.len(),
                    buffer_range: (offset..buffer_range.end).to_point(&buffer),
                    status: None,
                    excerpt: Some(excerpt.clone()),
                    deleted_hunk_anchor: None,
                });
            } else {
                let diff = self.diffs.get(&buffer_id).unwrap().read(cx).snapshot(cx);
                let base_buffer = diff.base_text();

                let mut offset = buffer_range.start;
                let hunks = diff
                    .hunks_intersecting_range(excerpt.range.clone(), buffer)
                    .peekable();

                for hunk in hunks {
                    // Ignore hunks that are outside the excerpt range.
                    let mut hunk_range = hunk.buffer_range.to_offset(buffer);

                    hunk_range.end = hunk_range.end.min(buffer_range.end);
                    if hunk_range.start > buffer_range.end || hunk_range.start < buffer_range.start
                    {
                        log::trace!("skipping hunk outside excerpt range");
                        continue;
                    }

                    if !self
                        .expanded_diff_hunks_by_buffer
                        .get(&buffer_id)
                        .cloned()
                        .into_iter()
                        .flatten()
                        .any(|expanded_anchor| {
                            expanded_anchor
                                .cmp(&hunk.buffer_range.start, buffer)
                                .is_eq()
                        })
                    {
                        log::trace!("skipping a hunk that's not marked as expanded");
                        continue;
                    }

                    if !hunk.buffer_range.start.is_valid(buffer) {
                        log::trace!("skipping hunk with deleted start: {:?}", hunk.range);
                        continue;
                    }

                    if hunk_range.start >= offset {
                        // Add the buffer text before the hunk
                        let len = text.len();
                        text.extend(buffer.text_for_range(offset..hunk_range.start));
                        if text.len() > len {
                            regions.push(ReferenceRegion {
                                buffer_id: Some(buffer_id),
                                range: len..text.len(),
                                buffer_range: (offset..hunk_range.start).to_point(&buffer),
                                status: None,
                                excerpt: Some(excerpt.clone()),
                                deleted_hunk_anchor: None,
                            });
                        }

                        // Add the deleted text for the hunk.
                        if !hunk.diff_base_byte_range.is_empty() {
                            let mut base_text = base_buffer
                                .text_for_range(hunk.diff_base_byte_range.clone())
                                .collect::<String>();
                            if !base_text.ends_with('\n') {
                                base_text.push('\n');
                            }
                            let len = text.len();
                            text.push_str(&base_text);
                            regions.push(ReferenceRegion {
                                buffer_id: Some(base_buffer.remote_id()),
                                range: len..text.len(),
                                buffer_range: hunk.diff_base_byte_range.to_point(&base_buffer),
                                status: Some(DiffHunkStatus::deleted(hunk.secondary_status)),
                                excerpt: Some(excerpt.clone()),
                                deleted_hunk_anchor: Some(hunk.buffer_range.start),
                            });
                        }

                        offset = hunk_range.start;
                    }

                    // Add the inserted text for the hunk.
                    if hunk_range.end > offset {
                        let len = text.len();
                        text.extend(buffer.text_for_range(offset..hunk_range.end));
                        let range = len..text.len();
                        let region = ReferenceRegion {
                            buffer_id: Some(buffer_id),
                            range,
                            buffer_range: (offset..hunk_range.end).to_point(&buffer),
                            status: Some(DiffHunkStatus::added(hunk.secondary_status)),
                            excerpt: Some(excerpt.clone()),
                            deleted_hunk_anchor: None,
                        };
                        offset = hunk_range.end;
                        regions.push(region);
                    }
                }

                // Add the buffer text for the rest of the excerpt.
                let len = text.len();
                text.extend(buffer.text_for_range(offset..buffer_range.end));
                text.push('\n');
                regions.push(ReferenceRegion {
                    buffer_id: Some(buffer_id),
                    range: len..text.len(),
                    buffer_range: (offset..buffer_range.end).to_point(&buffer),
                    status: None,
                    excerpt: Some(excerpt.clone()),
                    deleted_hunk_anchor: None,
                });
            }
        }

        // Remove final trailing newline.
        if self.excerpts.is_empty() {
            regions.push(ReferenceRegion {
                buffer_id: None,
                range: 0..1,
                buffer_range: Point::new(0, 0)..Point::new(0, 1),
                status: None,
                excerpt: None,
                deleted_hunk_anchor: None,
            });
        } else {
            text.pop();
            let region = regions.last_mut().unwrap();
            assert!(region.deleted_hunk_anchor.is_none());
            region.range.end -= 1;
        }

        // Retrieve the row info using the region that contains
        // the start of each multi-buffer line.
        let mut ix = 0;
        let row_infos = text
            .split('\n')
            .map(|line| {
                let row_info = regions
                    .iter()
                    .rposition(|region| {
                        region.range.contains(&ix) || (ix == text.len() && ix == region.range.end)
                    })
                    .map_or(RowInfo::default(), |region_ix| {
                        let region = regions[region_ix].clone();
                        let buffer_row = region.buffer_range.start.row
                            + text[region.range.start..ix].matches('\n').count() as u32;
                        let main_buffer = region.excerpt.as_ref().map(|e| e.buffer.clone());
                        let excerpt_range = region.excerpt.as_ref().map(|e| &e.range);
                        let is_excerpt_start = region_ix == 0
                            || regions[region_ix - 1].excerpt.as_ref().map(|e| &e.range)
                                != excerpt_range
                            || regions[region_ix - 1].range.is_empty();
                        let mut is_excerpt_end = region_ix == regions.len() - 1
                            || regions[region_ix + 1].excerpt.as_ref().map(|e| &e.range)
                                != excerpt_range;
                        let is_start = !text[region.range.start..ix].contains('\n');
                        let is_last_region = region_ix == regions.len() - 1;
                        let mut is_end = if region.range.end > text.len() {
                            !text[ix..].contains('\n')
                        } else {
                            let remaining_newlines = text[ix..region.range.end.min(text.len())]
                                .matches('\n')
                                .count();
                            remaining_newlines == if is_last_region { 0 } else { 1 }
                        };
                        if region_ix < regions.len() - 1
                            && !text[ix..].contains("\n")
                            && (region.status == Some(DiffHunkStatus::added_none())
                                || region.status.is_some_and(|s| s.is_deleted()))
                            && regions[region_ix + 1].excerpt.as_ref().map(|e| &e.range)
                                == excerpt_range
                            && regions[region_ix + 1].range.start == text.len()
                        {
                            is_end = true;
                            is_excerpt_end = true;
                        }
                        let multibuffer_row =
                            MultiBufferRow(text[..ix].matches('\n').count() as u32);
                        let mut expand_direction = None;
                        if let Some(buffer) = &main_buffer {
                            let needs_expand_up = is_excerpt_start && is_start && buffer_row > 0;
                            let needs_expand_down = is_excerpt_end
                                && is_end
                                && buffer.read(cx).max_point().row > buffer_row;
                            expand_direction = if needs_expand_up && needs_expand_down {
                                Some(ExpandExcerptDirection::UpAndDown)
                            } else if needs_expand_up {
                                Some(ExpandExcerptDirection::Up)
                            } else if needs_expand_down {
                                Some(ExpandExcerptDirection::Down)
                            } else {
                                None
                            };
                        }
                        RowInfo {
                            buffer_id: region.buffer_id,
                            diff_status: region.status,
                            buffer_row: Some(buffer_row),
                            wrapped_buffer_row: None,

                            multibuffer_row: Some(multibuffer_row),
                            expand_info: maybe!({
                                let direction = expand_direction?;
                                let excerpt = region.excerpt.as_ref()?;
                                Some(ExpandInfo {
                                    direction,
                                    start_anchor: Anchor::in_buffer(
                                        excerpt.path_key_index,
                                        excerpt.range.start,
                                    ),
                                })
                            }),
                        }
                    });
                ix += line.len() + 1;
                row_info
            })
            .collect();

        (text, row_infos, excerpt_boundary_rows, regions)
    }

    fn diffs_updated(&mut self, cx: &mut App) {
        let buffer_ids = self.diffs.keys().copied().collect::<Vec<_>>();
        for buffer_id in buffer_ids {
            self.update_expanded_diff_hunks_for_buffer(buffer_id, cx);
        }
    }

    fn add_diff(&mut self, diff: Entity<BufferDiff>, cx: &mut App) {
        let buffer_id = diff.read(cx).buffer_id;
        self.diffs.insert(buffer_id, diff);
    }

    fn add_inverted_diff(
        &mut self,
        diff: Entity<BufferDiff>,
        main_buffer: Entity<language::Buffer>,
        cx: &App,
    ) {
        let base_text_buffer_id = diff.read(cx).base_text(cx).remote_id();
        self.inverted_diffs
            .insert(base_text_buffer_id, (diff, main_buffer));
    }

    fn update_expanded_diff_hunks_for_buffer(&mut self, buffer_id: BufferId, cx: &mut App) {
        let excerpts = self
            .excerpts
            .iter()
            .filter(|excerpt| excerpt.buffer.read(cx).remote_id() == buffer_id)
            .collect::<Vec<_>>();
        let Some(buffer) = excerpts.first().map(|excerpt| excerpt.buffer.clone()) else {
            self.expanded_diff_hunks_by_buffer.remove(&buffer_id);
            return;
        };
        let buffer_snapshot = buffer.read(cx).snapshot();
        let Some(diff) = self.diffs.get(&buffer_id) else {
            self.expanded_diff_hunks_by_buffer.remove(&buffer_id);
            return;
        };
        let diff = diff.read(cx).snapshot(cx);
        let hunks = diff
            .hunks_in_row_range(0..u32::MAX, &buffer_snapshot)
            .collect::<Vec<_>>();
        self.expanded_diff_hunks_by_buffer
            .entry(buffer_id)
            .or_default()
            .retain(|hunk_anchor| {
                if !hunk_anchor.is_valid(&buffer_snapshot) {
                    return false;
                }

                let Ok(ix) = hunks.binary_search_by(|hunk| {
                    hunk.buffer_range.start.cmp(hunk_anchor, &buffer_snapshot)
                }) else {
                    return false;
                };
                let hunk_range = hunks[ix].buffer_range.to_point(&buffer_snapshot);
                excerpts.iter().any(|excerpt| {
                    let excerpt_range = excerpt.range.to_point(&buffer_snapshot);
                    hunk_range.start >= excerpt_range.start && hunk_range.start <= excerpt_range.end
                })
            });
    }

    fn anchor_to_offset(&self, anchor: &Anchor, cx: &App) -> Option<MultiBufferOffset> {
        if anchor.diff_base_anchor().is_some() {
            panic!("reference multibuffer cannot yet resolve anchors inside deleted hunks");
        }
        let (anchor, snapshot, path_key) = self.anchor_to_buffer_anchor(anchor, cx)?;
        // TODO(cole) can maybe make this and expected content call a common function instead
        let (text, _, _, regions) = self.expected_content(cx);

        // Locate the first region that contains or is past the putative location of the buffer anchor
        let ix = regions.partition_point(|region| {
            let excerpt = region
                .excerpt
                .as_ref()
                .expect("should have no buffers in empty reference multibuffer");
            excerpt
                .path_key
                .cmp(&path_key)
                .then_with(|| {
                    if excerpt.range.end.cmp(&anchor, &snapshot).is_lt() {
                        Ordering::Less
                    } else if excerpt.range.start.cmp(&anchor, &snapshot).is_gt() {
                        Ordering::Greater
                    } else {
                        Ordering::Equal
                    }
                })
                .then_with(|| {
                    if let Some(deleted_hunk_anchor) = region.deleted_hunk_anchor {
                        deleted_hunk_anchor.cmp(&anchor, &snapshot)
                    } else {
                        let point = anchor.to_point(&snapshot);
                        assert_eq!(region.buffer_id, Some(snapshot.remote_id()));
                        if region.buffer_range.end < point {
                            Ordering::Less
                        } else if region.buffer_range.start > point {
                            Ordering::Greater
                        } else {
                            Ordering::Equal
                        }
                    }
                })
                .is_lt()
        });

        let Some(region) = regions.get(ix) else {
            return Some(MultiBufferOffset(text.len()));
        };

        let offset = if region.buffer_id == Some(snapshot.remote_id()) {
            let buffer_offset = anchor.to_offset(&snapshot);
            let buffer_range = region.buffer_range.to_offset(&snapshot);
            assert!(buffer_offset <= buffer_range.end);
            let overshoot = buffer_offset.saturating_sub(buffer_range.start);
            region.range.start + overshoot
        } else {
            region.range.start
        };
        Some(MultiBufferOffset(offset))
    }

    fn anchor_to_buffer_anchor(
        &self,
        anchor: &Anchor,
        cx: &App,
    ) -> Option<(text::Anchor, BufferSnapshot, PathKey)> {
        let (excerpt, anchor) = match anchor {
            Anchor::Min => {
                let excerpt = self.excerpts.first()?;
                (excerpt, excerpt.range.start)
            }
            Anchor::Excerpt(excerpt_anchor) => (
                self.excerpts.iter().find(|excerpt| {
                    excerpt.buffer.read(cx).remote_id() == excerpt_anchor.buffer_id()
                })?,
                excerpt_anchor.text_anchor,
            ),
            Anchor::Max => {
                let excerpt = self.excerpts.last()?;
                (excerpt, excerpt.range.end)
            }
        };

        Some((
            anchor,
            excerpt.buffer.read(cx).snapshot(),
            excerpt.path_key.clone(),
        ))
    }
}

#[gpui::test(iterations = 100)]
async fn test_random_set_ranges(cx: &mut TestAppContext, mut rng: StdRng) {
    let base_text = "a\n".repeat(100);
    let buf = cx.update(|cx| cx.new(|cx| Buffer::local(base_text, cx)));
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    fn row_ranges(ranges: &Vec<Range<Point>>) -> Vec<Range<u32>> {
        ranges
            .iter()
            .map(|range| range.start.row..range.end.row)
            .collect()
    }

    for _ in 0..operations {
        let snapshot = buf.update(cx, |buf, _| buf.snapshot());
        let num_ranges = rng.random_range(0..=10);
        let max_row = snapshot.max_point().row;
        let mut ranges = (0..num_ranges)
            .map(|_| {
                let start = rng.random_range(0..max_row);
                let end = rng.random_range(start + 1..max_row + 1);
                Point::row_range(start..end)
            })
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| range.start);
        log::info!("Setting ranges: {:?}", row_ranges(&ranges));
        multibuffer.update(cx, |multibuffer, cx| {
            multibuffer.set_excerpts_for_path(
                PathKey::for_buffer(&buf, cx),
                buf.clone(),
                ranges.clone(),
                2,
                cx,
            )
        });

        let snapshot = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
        let mut last_end = None;
        let mut seen_ranges = Vec::default();

        for info in snapshot.excerpts() {
            let buffer_snapshot = snapshot
                .buffer_for_id(info.context.start.buffer_id)
                .unwrap();
            let start = info.context.start.to_point(buffer_snapshot);
            let end = info.context.end.to_point(buffer_snapshot);
            seen_ranges.push(start..end);

            if let Some(last_end) = last_end.take() {
                assert!(
                    start > last_end,
                    "multibuffer has out-of-order ranges: {:?}; {:?} <= {:?}",
                    row_ranges(&seen_ranges),
                    start,
                    last_end
                )
            }

            ranges.retain(|range| range.start < start || range.end > end);

            last_end = Some(end)
        }

        assert!(
            ranges.is_empty(),
            "multibuffer {:?} did not include all ranges: {:?}",
            row_ranges(&seen_ranges),
            row_ranges(&ranges)
        );
    }
}

#[gpui::test(iterations = 100)]
async fn test_random_multibuffer(cx: &mut TestAppContext, mut rng: StdRng) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let mut buffers: Vec<Entity<Buffer>> = Vec::new();
    let mut base_texts: HashMap<BufferId, String> = HashMap::default();
    let mut reference = ReferenceMultibuffer::default();
    let mut anchors = Vec::new();
    let mut old_versions = Vec::new();
    let mut needs_diff_calculation = false;
    let mut inverted_diff_main_buffers: HashMap<BufferId, Entity<BufferDiff>> = HashMap::default();
    for _ in 0..operations {
        match rng.random_range(0..100) {
            0..=14 if !buffers.is_empty() => {
                let buffer = buffers.choose(&mut rng).unwrap();
                buffer.update(cx, |buf, cx| {
                    let edit_count = rng.random_range(1..5);
                    buf.randomly_edit(&mut rng, edit_count, cx);
                    log::info!("buffer text:\n{}", buf.text());
                    needs_diff_calculation = true;
                });
                cx.update(|cx| reference.diffs_updated(cx));
            }
            15..=24 if !reference.excerpts.is_empty() => {
                multibuffer.update(cx, |multibuffer, cx| {
                    let snapshot = multibuffer.snapshot(cx);
                    let infos = snapshot.excerpts().collect::<Vec<_>>();
                    let mut excerpts = HashSet::default();
                    for _ in 0..rng.random_range(0..infos.len()) {
                        excerpts.extend(infos.choose(&mut rng).cloned());
                    }

                    let line_count = rng.random_range(0..5);

                    let excerpt_ixs = excerpts
                        .iter()
                        .map(|info| {
                            reference
                                .excerpts
                                .iter()
                                .position(|e| e.range == info.context)
                                .unwrap()
                        })
                        .collect::<Vec<_>>();
                    log::info!("Expanding excerpts {excerpt_ixs:?} by {line_count} lines");
                    multibuffer.expand_excerpts(
                        excerpts
                            .iter()
                            .map(|info| snapshot.anchor_in_excerpt(info.context.end).unwrap()),
                        line_count,
                        ExpandExcerptDirection::UpAndDown,
                        cx,
                    );

                    reference.expand_excerpts(&excerpts, line_count, cx);
                });
            }
            25..=34 if !reference.excerpts.is_empty() => {
                let multibuffer =
                    multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
                let offset = multibuffer.clip_offset(
                    MultiBufferOffset(rng.random_range(0..=multibuffer.len().0)),
                    Bias::Left,
                );
                let bias = if rng.random() {
                    Bias::Left
                } else {
                    Bias::Right
                };
                log::info!("Creating anchor at {} with bias {:?}", offset.0, bias);
                anchors.push(multibuffer.anchor_at(offset, bias));
                anchors.sort_by(|a, b| a.cmp(b, &multibuffer));
            }
            35..=45 if !reference.excerpts.is_empty() => {
                multibuffer.update(cx, |multibuffer, cx| {
                    let snapshot = multibuffer.snapshot(cx);
                    let excerpt_ix = rng.random_range(0..reference.excerpts.len());
                    let excerpt = &reference.excerpts[excerpt_ix];

                    // Skip inverted excerpts - hunks can't be collapsed
                    let buffer_id = excerpt.buffer.read(cx).remote_id();
                    if reference.inverted_diffs.contains_key(&buffer_id) {
                        return;
                    }

                    let start = excerpt.range.start;
                    let end = excerpt.range.end;
                    let range = snapshot.anchor_in_excerpt(start).unwrap()
                        ..snapshot.anchor_in_excerpt(end).unwrap();

                    log::info!(
                        "expanding diff hunks in range {:?} (excerpt index {excerpt_ix:?}, buffer id {:?})",
                        range.to_point(&snapshot),
                        buffer_id,
                    );
                    reference.expand_diff_hunks(excerpt.path_key.clone(), start..end, cx);
                    multibuffer.expand_diff_hunks(vec![range], cx);
                });
            }
            46..=75 if needs_diff_calculation => {
                multibuffer.update(cx, |multibuffer, cx| {
                    for buffer in multibuffer.all_buffers() {
                        let snapshot = buffer.read(cx).snapshot();
                        let buffer_id = snapshot.remote_id();

                        if let Some(diff) = multibuffer.diff_for(buffer_id) {
                            diff.update(cx, |diff, cx| {
                                log::info!("recalculating diff for buffer {:?}", buffer_id,);
                                diff.recalculate_diff_sync(&snapshot.text, cx);
                            });
                        }

                        if let Some(inverted_diff) = inverted_diff_main_buffers.get(&buffer_id) {
                            inverted_diff.update(cx, |diff, cx| {
                                log::info!(
                                    "recalculating inverted diff for main buffer {:?}",
                                    buffer_id,
                                );
                                diff.recalculate_diff_sync(&snapshot.text, cx);
                            });
                        }
                    }
                    reference.diffs_updated(cx);
                    needs_diff_calculation = false;
                });
            }
            _ => {
                // Decide if we're creating a new buffer or reusing an existing one
                let create_new_buffer = buffers.is_empty() || rng.random_bool(0.4);

                let (excerpt_buffer, diff, inverted_main_buffer) = if create_new_buffer {
                    let create_inverted = rng.random_bool(0.3);

                    if create_inverted {
                        let mut main_buffer_text = util::RandomCharIter::new(&mut rng)
                            .take(256)
                            .collect::<String>();
                        let main_buffer = cx.new(|cx| Buffer::local(main_buffer_text.clone(), cx));
                        text::LineEnding::normalize(&mut main_buffer_text);
                        let main_buffer_id =
                            main_buffer.read_with(cx, |buffer, _| buffer.remote_id());
                        base_texts.insert(main_buffer_id, main_buffer_text.clone());
                        buffers.push(main_buffer.clone());

                        let diff = cx.new(|cx| {
                            BufferDiff::new_with_base_text(
                                &main_buffer_text,
                                &main_buffer.read(cx).text_snapshot(),
                                cx,
                            )
                        });

                        let base_text_buffer =
                            diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());

                        // Track for recalculation when main buffer is edited
                        inverted_diff_main_buffers.insert(main_buffer_id, diff.clone());

                        (base_text_buffer, diff, Some(main_buffer))
                    } else {
                        let mut base_text = util::RandomCharIter::new(&mut rng)
                            .take(256)
                            .collect::<String>();

                        let buffer_handle = cx.new(|cx| Buffer::local(base_text.clone(), cx));
                        text::LineEnding::normalize(&mut base_text);
                        let buffer_id = buffer_handle.read_with(cx, |buffer, _| buffer.remote_id());
                        base_texts.insert(buffer_id, base_text.clone());
                        buffers.push(buffer_handle.clone());

                        let diff = cx.new(|cx| {
                            BufferDiff::new_with_base_text(
                                &base_text,
                                &buffer_handle.read(cx).text_snapshot(),
                                cx,
                            )
                        });

                        (buffer_handle, diff, None)
                    }
                } else {
                    // Reuse an existing buffer
                    let buffer_handle = buffers.choose(&mut rng).unwrap().clone();
                    let buffer_id = buffer_handle.read_with(cx, |buffer, _| buffer.remote_id());

                    if let Some(diff) = inverted_diff_main_buffers.get(&buffer_id) {
                        let base_text_buffer =
                            diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());
                        (base_text_buffer, diff.clone(), Some(buffer_handle))
                    } else {
                        // Get existing diff or create new one for regular buffer
                        let diff = multibuffer
                            .read_with(cx, |mb, _| mb.diff_for(buffer_id))
                            .unwrap_or_else(|| {
                                let base_text = base_texts.get(&buffer_id).unwrap();
                                cx.new(|cx| {
                                    BufferDiff::new_with_base_text(
                                        base_text,
                                        &buffer_handle.read(cx).text_snapshot(),
                                        cx,
                                    )
                                })
                            });
                        (buffer_handle, diff, None)
                    }
                };

                let excerpt_buffer_snapshot =
                    excerpt_buffer.read_with(cx, |excerpt_buffer, _| excerpt_buffer.snapshot());
                let mut ranges = reference
                    .excerpts
                    .iter()
                    .filter(|excerpt| excerpt.buffer == excerpt_buffer)
                    .map(|excerpt| excerpt.range.to_point(&excerpt_buffer_snapshot))
                    .collect::<Vec<_>>();
                mutate_excerpt_ranges(&mut rng, &mut ranges, &excerpt_buffer_snapshot, 1);
                let ranges = ranges
                    .iter()
                    .cloned()
                    .map(ExcerptRange::new)
                    .collect::<Vec<_>>();
                let path = cx.update(|cx| PathKey::for_buffer(&excerpt_buffer, cx));
                let path_key_index = multibuffer.update(cx, |multibuffer, _| {
                    multibuffer.get_or_create_path_key_index(&path)
                });

                multibuffer.update(cx, |multibuffer, cx| {
                    multibuffer.set_excerpt_ranges_for_path(
                        path.clone(),
                        excerpt_buffer.clone(),
                        &excerpt_buffer_snapshot,
                        ranges.clone(),
                        cx,
                    )
                });

                cx.update(|cx| {
                    reference.set_excerpts(
                        path,
                        path_key_index,
                        excerpt_buffer.clone(),
                        &excerpt_buffer_snapshot,
                        ranges,
                        cx,
                    )
                });

                let excerpt_buffer_id =
                    excerpt_buffer.read_with(cx, |buffer, _| buffer.remote_id());
                multibuffer.update(cx, |multibuffer, cx| {
                    if multibuffer.diff_for(excerpt_buffer_id).is_none() {
                        if let Some(main_buffer) = inverted_main_buffer {
                            reference.add_inverted_diff(diff.clone(), main_buffer.clone(), cx);
                            multibuffer.add_inverted_diff(diff, main_buffer, cx);
                        } else {
                            reference.add_diff(diff.clone(), cx);
                            multibuffer.add_diff(diff, cx);
                        }
                    }
                });
            }
        }

        if rng.random_bool(0.3) {
            multibuffer.update(cx, |multibuffer, cx| {
                old_versions.push((multibuffer.snapshot(cx), multibuffer.subscribe()));
            })
        }

        multibuffer.read_with(cx, |multibuffer, cx| {
            check_multibuffer(multibuffer, &reference, &anchors, cx, &mut rng);
        });
    }
    let snapshot = multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    for (old_snapshot, subscription) in old_versions {
        check_multibuffer_edits(&snapshot, &old_snapshot, subscription);
    }
}

fn mutate_excerpt_ranges(
    rng: &mut StdRng,
    existing_ranges: &mut Vec<Range<Point>>,
    buffer: &BufferSnapshot,
    operations: u32,
) {
    let mut ranges_to_add = Vec::new();

    for _ in 0..operations {
        match rng.random_range(0..5) {
            0..=1 if !existing_ranges.is_empty() => {
                let index = rng.random_range(0..existing_ranges.len());
                log::info!("Removing excerpt at index {index}");
                existing_ranges.remove(index);
            }
            _ => {
                let end_row = rng.random_range(0..=buffer.max_point().row);
                let start_row = rng.random_range(0..=end_row);
                let end_col = buffer.line_len(end_row);
                log::info!(
                    "Inserting excerpt for buffer {:?}, row range {:?}",
                    buffer.remote_id(),
                    start_row..end_row
                );
                ranges_to_add.push(Point::new(start_row, 0)..Point::new(end_row, end_col));
            }
        }
    }

    existing_ranges.extend(ranges_to_add);
    existing_ranges.sort_by_key(|r| r.start);
}

fn check_multibuffer(
    multibuffer: &MultiBuffer,
    reference: &ReferenceMultibuffer,
    anchors: &[Anchor],
    cx: &App,
    rng: &mut StdRng,
) {
    let snapshot = multibuffer.snapshot(cx);
    let actual_text = snapshot.text();
    let actual_boundary_rows = snapshot
        .excerpt_boundaries_in_range(MultiBufferOffset(0)..)
        .map(|b| b.row)
        .collect::<HashSet<_>>();
    let actual_row_infos = snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>();

    let anchors_to_check = anchors
        .iter()
        .filter_map(|anchor| {
            snapshot
                .anchor_to_buffer_anchor(*anchor)
                .map(|(anchor, _)| anchor)
        })
        // Intentionally mix in some anchors that are (in general) not contained in any excerpt
        .chain(
            reference
                .excerpts
                .iter()
                .map(|excerpt| excerpt.buffer.read(cx).remote_id())
                .dedup()
                .flat_map(|buffer_id| {
                    [
                        text::Anchor::min_for_buffer(buffer_id),
                        text::Anchor::max_for_buffer(buffer_id),
                    ]
                }),
        )
        .map(|anchor| snapshot.anchor_in_buffer(anchor).unwrap())
        .collect::<Vec<_>>();

    let (expected_text, expected_row_infos, expected_boundary_rows, _) =
        reference.expected_content(cx);
    let expected_anchor_offsets = anchors_to_check
        .iter()
        .map(|anchor| reference.anchor_to_offset(anchor, cx).unwrap())
        .collect::<Vec<_>>();

    let has_diff = actual_row_infos
        .iter()
        .any(|info| info.diff_status.is_some())
        || expected_row_infos
            .iter()
            .any(|info| info.diff_status.is_some());
    let actual_diff = format_diff(
        &actual_text,
        &actual_row_infos,
        &actual_boundary_rows,
        Some(has_diff),
    );
    let expected_diff = format_diff(
        &expected_text,
        &expected_row_infos,
        &expected_boundary_rows,
        Some(has_diff),
    );

    log::info!("Multibuffer content:\n{}", actual_diff);

    assert_eq!(
        actual_row_infos.len(),
        actual_text.split('\n').count(),
        "line count: {}",
        actual_text.split('\n').count()
    );
    pretty_assertions::assert_eq!(actual_diff, expected_diff);
    pretty_assertions::assert_eq!(actual_text, expected_text);
    pretty_assertions::assert_eq!(actual_row_infos, expected_row_infos);

    for _ in 0..5 {
        let start_row = rng.random_range(0..=expected_row_infos.len());
        assert_eq!(
            snapshot
                .row_infos(MultiBufferRow(start_row as u32))
                .collect::<Vec<_>>(),
            &expected_row_infos[start_row..],
            "buffer_rows({})",
            start_row
        );
    }

    assert_eq!(
        snapshot.widest_line_number(),
        expected_row_infos
            .into_iter()
            .filter_map(|info| {
                // For inverted diffs, deleted rows are visible and should be counted.
                // Only filter out deleted rows that are NOT from inverted diffs.
                let is_inverted_diff = info
                    .buffer_id
                    .is_some_and(|id| reference.inverted_diffs.contains_key(&id));
                if info.diff_status.is_some_and(|status| status.is_deleted()) && !is_inverted_diff {
                    None
                } else {
                    info.buffer_row
                }
            })
            .max()
            .unwrap()
            + 1
    );
    for i in 0..snapshot.len().0 {
        let (_, excerpt_range) = snapshot
            .excerpt_containing(MultiBufferOffset(i)..MultiBufferOffset(i))
            .unwrap();
        reference
            .excerpts
            .iter()
            .find(|reference_excerpt| reference_excerpt.range == excerpt_range.context)
            .expect("corresponding excerpt should exist in reference multibuffer");
    }

    assert_consistent_line_numbers(&snapshot);
    assert_position_translation(&snapshot);

    for (row, line) in expected_text.split('\n').enumerate() {
        assert_eq!(
            snapshot.line_len(MultiBufferRow(row as u32)),
            line.len() as u32,
            "line_len({}).",
            row
        );
    }

    let text_rope = Rope::from(expected_text.as_str());
    for _ in 0..10 {
        let end_ix = text_rope.clip_offset(rng.random_range(0..=text_rope.len()), Bias::Right);
        let start_ix = text_rope.clip_offset(rng.random_range(0..=end_ix), Bias::Left);

        let text_for_range = snapshot
            .text_for_range(MultiBufferOffset(start_ix)..MultiBufferOffset(end_ix))
            .collect::<String>();
        assert_eq!(
            text_for_range,
            &expected_text[start_ix..end_ix],
            "incorrect text for range {:?}",
            start_ix..end_ix
        );

        let expected_summary =
            MBTextSummary::from(TextSummary::from(&expected_text[start_ix..end_ix]));
        assert_eq!(
            snapshot.text_summary_for_range::<MBTextSummary, _>(
                MultiBufferOffset(start_ix)..MultiBufferOffset(end_ix)
            ),
            expected_summary,
            "incorrect summary for range {:?}",
            start_ix..end_ix
        );
    }

    // Anchor resolution
    let summaries = snapshot.summaries_for_anchors::<MultiBufferOffset, _>(anchors);
    assert_eq!(anchors.len(), summaries.len());
    for (anchor, resolved_offset) in anchors.iter().zip(summaries) {
        assert!(resolved_offset <= snapshot.len());
        assert_eq!(
            snapshot.summary_for_anchor::<MultiBufferOffset>(anchor),
            resolved_offset,
            "anchor: {:?}",
            anchor
        );
    }

    let actual_anchor_offsets = anchors_to_check
        .into_iter()
        .map(|anchor| anchor.to_offset(&snapshot))
        .collect::<Vec<_>>();
    assert_eq!(
        actual_anchor_offsets, expected_anchor_offsets,
        "buffer anchor resolves to wrong offset"
    );

    for _ in 0..10 {
        let end_ix = text_rope.clip_offset(rng.random_range(0..=text_rope.len()), Bias::Right);
        assert_eq!(
            snapshot
                .reversed_chars_at(MultiBufferOffset(end_ix))
                .collect::<String>(),
            expected_text[..end_ix].chars().rev().collect::<String>(),
        );
    }

    for _ in 0..10 {
        let end_ix = rng.random_range(0..=text_rope.len());
        let end_ix = text_rope.floor_char_boundary(end_ix);
        let start_ix = rng.random_range(0..=end_ix);
        let start_ix = text_rope.floor_char_boundary(start_ix);
        assert_eq!(
            snapshot
                .bytes_in_range(MultiBufferOffset(start_ix)..MultiBufferOffset(end_ix))
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
            expected_text.as_bytes()[start_ix..end_ix].to_vec(),
            "bytes_in_range({:?})",
            start_ix..end_ix,
        );
    }
}

fn check_multibuffer_edits(
    snapshot: &MultiBufferSnapshot,
    old_snapshot: &MultiBufferSnapshot,
    subscription: Subscription<MultiBufferOffset>,
) {
    let edits = subscription.consume().into_inner();

    log::info!(
        "applying subscription edits to old text: {:?}: {:#?}",
        old_snapshot.text(),
        edits,
    );

    let mut text = old_snapshot.text();
    for edit in edits {
        let new_text: String = snapshot
            .text_for_range(edit.new.start..edit.new.end)
            .collect();
        text.replace_range(
            (edit.new.start.0..edit.new.start.0 + (edit.old.end.0 - edit.old.start.0)).clone(),
            &new_text,
        );
        pretty_assertions::assert_eq!(
            &text[0..edit.new.end.0],
            snapshot
                .text_for_range(MultiBufferOffset(0)..edit.new.end)
                .collect::<String>()
        );
    }
    pretty_assertions::assert_eq!(text, snapshot.text());
}

fn format_diff(
    text: &str,
    row_infos: &Vec<RowInfo>,
    boundary_rows: &HashSet<MultiBufferRow>,
    has_diff: Option<bool>,
) -> String {
    let has_diff =
        has_diff.unwrap_or_else(|| row_infos.iter().any(|info| info.diff_status.is_some()));
    text.split('\n')
        .enumerate()
        .zip(row_infos)
        .map(|((ix, line), info)| {
            let marker = match info.diff_status.map(|status| status.kind) {
                Some(DiffHunkStatusKind::Added) => "+ ",
                Some(DiffHunkStatusKind::Deleted) => "- ",
                Some(DiffHunkStatusKind::Modified) => unreachable!(),
                None => {
                    if has_diff && !line.is_empty() {
                        "  "
                    } else {
                        ""
                    }
                }
            };
            let boundary_row = if boundary_rows.contains(&MultiBufferRow(ix as u32)) {
                if has_diff {
                    "  ----------\n"
                } else {
                    "---------\n"
                }
            } else {
                ""
            };
            let expand = info
                .expand_info
                .as_ref()
                .map(|expand_info| match expand_info.direction {
                    ExpandExcerptDirection::Up => " [↑]",
                    ExpandExcerptDirection::Down => " [↓]",
                    ExpandExcerptDirection::UpAndDown => " [↕]",
                })
                .unwrap_or_default();

            format!("{boundary_row}{marker}{line}{expand}")
            // let mbr = info
            //     .multibuffer_row
            //     .map(|row| format!("{:0>3}", row.0))
            //     .unwrap_or_else(|| "???".to_string());
            // let byte_range = format!("{byte_range_start:0>3}..{byte_range_end:0>3}");
            // format!("{boundary_row}Row: {mbr}, Bytes: {byte_range} | {marker}{line}{expand}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// fn format_transforms(snapshot: &MultiBufferSnapshot) -> String {
//     snapshot
//         .diff_transforms
//         .iter()
//         .map(|transform| {
//             let (kind, summary) = match transform {
//                 DiffTransform::DeletedHunk { summary, .. } => ("   Deleted", (*summary).into()),
//                 DiffTransform::FilteredInsertedHunk { summary, .. } => ("  Filtered", *summary),
//                 DiffTransform::InsertedHunk { summary, .. } => ("  Inserted", *summary),
//                 DiffTransform::Unmodified { summary, .. } => ("Unmodified", *summary),
//             };
//             format!("{kind}(len: {}, lines: {:?})", summary.len, summary.lines)
//         })
//         .join("\n")
// }

// fn format_excerpts(snapshot: &MultiBufferSnapshot) -> String {
//     snapshot
//         .excerpts
//         .iter()
//         .map(|excerpt| {
//             format!(
//                 "Excerpt(buffer_range = {:?}, lines = {:?}, has_trailing_newline = {:?})",
//                 excerpt.range.context.to_point(&excerpt.buffer),
//                 excerpt.text_summary.lines,
//                 excerpt.has_trailing_newline
//             )
//         })
//         .join("\n")
// }

#[track_caller]
fn assert_excerpts_match(
    multibuffer: &Entity<MultiBuffer>,
    cx: &mut TestAppContext,
    expected: &str,
) {
    let mut output = String::new();
    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        for excerpt in multibuffer.snapshot(cx).excerpts() {
            output.push_str("-----\n");
            output.extend(
                snapshot
                    .buffer_for_id(excerpt.context.start.buffer_id)
                    .unwrap()
                    .text_for_range(excerpt.context),
            );
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }
    });
    assert_eq!(output, expected);
}

#[track_caller]
fn assert_new_snapshot(
    multibuffer: &Entity<MultiBuffer>,
    snapshot: &mut MultiBufferSnapshot,
    subscription: &mut Subscription<MultiBufferOffset>,
    cx: &mut TestAppContext,
    expected_diff: &str,
) {
    let new_snapshot = multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    let actual_text = new_snapshot.text();
    let line_infos = new_snapshot
        .row_infos(MultiBufferRow(0))
        .collect::<Vec<_>>();
    let actual_diff = format_diff(&actual_text, &line_infos, &Default::default(), None);
    pretty_assertions::assert_eq!(actual_diff, expected_diff);
    check_edits(
        snapshot,
        &new_snapshot,
        &subscription.consume().into_inner(),
    );
    *snapshot = new_snapshot;
}

#[track_caller]
fn check_edits(
    old_snapshot: &MultiBufferSnapshot,
    new_snapshot: &MultiBufferSnapshot,
    edits: &[Edit<MultiBufferOffset>],
) {
    let mut text = old_snapshot.text();
    let new_text = new_snapshot.text();
    for edit in edits.iter().rev() {
        if !text.is_char_boundary(edit.old.start.0)
            || !text.is_char_boundary(edit.old.end.0)
            || !new_text.is_char_boundary(edit.new.start.0)
            || !new_text.is_char_boundary(edit.new.end.0)
        {
            panic!(
                "invalid edits: {:?}\nold text: {:?}\nnew text: {:?}",
                edits, text, new_text
            );
        }

        text.replace_range(
            edit.old.start.0..edit.old.end.0,
            &new_text[edit.new.start.0..edit.new.end.0],
        );
    }

    pretty_assertions::assert_eq!(text, new_text, "invalid edits: {:?}", edits);
}

#[track_caller]
fn assert_chunks_in_ranges(snapshot: &MultiBufferSnapshot) {
    let full_text = snapshot.text();
    for ix in 0..full_text.len() {
        let mut chunks = snapshot.chunks(
            MultiBufferOffset(0)..snapshot.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
        );
        chunks.seek(MultiBufferOffset(ix)..snapshot.len());
        let tail = chunks.map(|chunk| chunk.text).collect::<String>();
        assert_eq!(tail, &full_text[ix..], "seek to range: {:?}", ix..);
    }
}

#[track_caller]
fn assert_consistent_line_numbers(snapshot: &MultiBufferSnapshot) {
    let all_line_numbers = snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>();
    for start_row in 1..all_line_numbers.len() {
        let line_numbers = snapshot
            .row_infos(MultiBufferRow(start_row as u32))
            .collect::<Vec<_>>();
        assert_eq!(
            line_numbers,
            all_line_numbers[start_row..],
            "start_row: {start_row}"
        );
    }
}

#[track_caller]
fn assert_position_translation(snapshot: &MultiBufferSnapshot) {
    let text = Rope::from(snapshot.text());

    let mut left_anchors = Vec::new();
    let mut right_anchors = Vec::new();
    let mut offsets = Vec::new();
    let mut points = Vec::new();
    for offset in 0..=text.len() + 1 {
        let offset = MultiBufferOffset(offset);
        let clipped_left = snapshot.clip_offset(offset, Bias::Left);
        let clipped_right = snapshot.clip_offset(offset, Bias::Right);
        assert_eq!(
            clipped_left.0,
            text.clip_offset(offset.0, Bias::Left),
            "clip_offset({offset:?}, Left)"
        );
        assert_eq!(
            clipped_right.0,
            text.clip_offset(offset.0, Bias::Right),
            "clip_offset({offset:?}, Right)"
        );
        assert_eq!(
            snapshot.offset_to_point(clipped_left),
            text.offset_to_point(clipped_left.0),
            "offset_to_point({})",
            clipped_left.0
        );
        assert_eq!(
            snapshot.offset_to_point(clipped_right),
            text.offset_to_point(clipped_right.0),
            "offset_to_point({})",
            clipped_right.0
        );
        let anchor_after = snapshot.anchor_after(clipped_left);
        assert_eq!(
            anchor_after.to_offset(snapshot),
            clipped_left,
            "anchor_after({}).to_offset {anchor_after:?}",
            clipped_left.0
        );
        let anchor_before = snapshot.anchor_before(clipped_left);
        assert_eq!(
            anchor_before.to_offset(snapshot),
            clipped_left,
            "anchor_before({}).to_offset",
            clipped_left.0
        );
        left_anchors.push(anchor_before);
        right_anchors.push(anchor_after);
        offsets.push(clipped_left);
        points.push(text.offset_to_point(clipped_left.0));
    }

    for row in 0..text.max_point().row {
        for column in 0..text.line_len(row) + 1 {
            let point = Point { row, column };
            let clipped_left = snapshot.clip_point(point, Bias::Left);
            let clipped_right = snapshot.clip_point(point, Bias::Right);
            assert_eq!(
                clipped_left,
                text.clip_point(point, Bias::Left),
                "clip_point({point:?}, Left)"
            );
            assert_eq!(
                clipped_right,
                text.clip_point(point, Bias::Right),
                "clip_point({point:?}, Right)"
            );
            assert_eq!(
                snapshot.point_to_offset(clipped_left).0,
                text.point_to_offset(clipped_left),
                "point_to_offset({clipped_left:?})"
            );
            assert_eq!(
                snapshot.point_to_offset(clipped_right).0,
                text.point_to_offset(clipped_right),
                "point_to_offset({clipped_right:?})"
            );
        }
    }

    assert_eq!(
        snapshot.summaries_for_anchors::<MultiBufferOffset, _>(&left_anchors),
        offsets,
        "left_anchors <-> offsets"
    );
    assert_eq!(
        snapshot.summaries_for_anchors::<Point, _>(&left_anchors),
        points,
        "left_anchors <-> points"
    );
    assert_eq!(
        snapshot.summaries_for_anchors::<MultiBufferOffset, _>(&right_anchors),
        offsets,
        "right_anchors <-> offsets"
    );
    assert_eq!(
        snapshot.summaries_for_anchors::<Point, _>(&right_anchors),
        points,
        "right_anchors <-> points"
    );

    for (anchors, bias) in [(&left_anchors, Bias::Left), (&right_anchors, Bias::Right)] {
        for (ix, (offset, anchor)) in offsets.iter().zip(anchors).enumerate() {
            if ix > 0 && *offset == MultiBufferOffset(252) && offset > &offsets[ix - 1] {
                let prev_anchor = left_anchors[ix - 1];
                assert!(
                    anchor.cmp(&prev_anchor, snapshot).is_gt(),
                    "anchor({}, {bias:?}).cmp(&anchor({}, {bias:?}).is_gt()",
                    offsets[ix],
                    offsets[ix - 1],
                );
                assert!(
                    prev_anchor.cmp(anchor, snapshot).is_lt(),
                    "anchor({}, {bias:?}).cmp(&anchor({}, {bias:?}).is_lt()",
                    offsets[ix - 1],
                    offsets[ix],
                );
            }
        }
    }

    if let Some((buffer, offset)) = snapshot.point_to_buffer_offset(snapshot.max_point()) {
        assert!(offset.0 <= buffer.len());
    }
    if let Some((buffer, point)) = snapshot.point_to_buffer_point(snapshot.max_point()) {
        assert!(point <= buffer.max_point());
    }
}

fn assert_line_indents(snapshot: &MultiBufferSnapshot) {
    let max_row = snapshot.max_point().row;
    let buffer_id = snapshot.excerpts().next().unwrap().context.start.buffer_id;
    let text = text::Buffer::new(ReplicaId::LOCAL, buffer_id, snapshot.text());
    let mut line_indents = text
        .line_indents_in_row_range(0..max_row + 1)
        .collect::<Vec<_>>();
    for start_row in 0..snapshot.max_point().row {
        pretty_assertions::assert_eq!(
            snapshot
                .line_indents(MultiBufferRow(start_row), |_| true)
                .map(|(row, indent, _)| (row.0, indent))
                .collect::<Vec<_>>(),
            &line_indents[(start_row as usize)..],
            "line_indents({start_row})"
        );
    }

    line_indents.reverse();
    pretty_assertions::assert_eq!(
        snapshot
            .reversed_line_indents(MultiBufferRow(max_row), |_| true)
            .map(|(row, indent, _)| (row.0, indent))
            .collect::<Vec<_>>(),
        &line_indents[..],
        "reversed_line_indents({max_row})"
    );
}

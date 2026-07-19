use std::{ops::Range, rc::Rc, sync::Arc};

use gpui::{App, AppContext, Context, Entity};
use itertools::Itertools;
use language::{Buffer, BufferEditSource, BufferSnapshot};
use rope::Point;
use sum_tree::{Dimensions, SumTree};
use text::{Bias, BufferId, Edit, OffsetRangeExt, Patch};
use util::rel_path::RelPath;
use ztracing::instrument;

use crate::{
    Anchor, BufferState, BufferStateSnapshot, DiffChangeKind, Event, Excerpt, ExcerptOffset,
    ExcerptRange, ExcerptSummary, ExpandExcerptDirection, MultiBuffer, MultiBufferOffset,
    PathKeyIndex, build_excerpt_ranges, remove_diff_state,
};

#[derive(PartialEq, Eq, Ord, PartialOrd, Clone, Hash, Debug)]
pub struct PathKey {
    // Used by the derived PartialOrd & Ord
    pub sort_prefix: Option<u64>,
    pub path: Arc<RelPath>,
}

impl PathKey {
    pub fn min() -> Self {
        Self {
            sort_prefix: None,
            path: RelPath::empty_arc(),
        }
    }

    pub fn sorted(sort_prefix: u64) -> Self {
        Self {
            sort_prefix: Some(sort_prefix),
            path: RelPath::empty_arc(),
        }
    }
    pub fn with_sort_prefix(sort_prefix: u64, path: Arc<RelPath>) -> Self {
        Self {
            sort_prefix: Some(sort_prefix),
            path,
        }
    }

    pub fn for_buffer(buffer: &Entity<Buffer>, cx: &App) -> Self {
        if let Some(file) = buffer.read(cx).file() {
            Self::with_sort_prefix(file.worktree_id(cx).to_proto(), file.path().clone())
        } else {
            Self {
                sort_prefix: None,
                path: RelPath::unix(&buffer.entity_id().to_string())
                    .unwrap()
                    .into_arc(),
            }
        }
    }
}

impl MultiBuffer {
    pub fn location_for_path(&self, path: &PathKey, cx: &App) -> Option<Anchor> {
        let snapshot = self.snapshot(cx);
        let excerpt = snapshot.excerpts_for_path(path).next()?;
        let path_key_index = snapshot.path_key_index_for_buffer(excerpt.context.start.buffer_id)?;
        Some(Anchor::in_buffer(path_key_index, excerpt.context.start))
    }

    pub fn set_excerpts_for_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        ranges: impl IntoIterator<Item = Range<Point>>,
        context_line_count: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let path = PathKey::for_buffer(&buffer, cx);
        self.set_excerpts_for_path(path, buffer, ranges, context_line_count, cx)
    }

    /// Sets excerpts, returns `true` if at least one new excerpt was added.
    ///
    /// Any existing excerpts for this buffer or this path will be replaced by the provided ranges.
    #[instrument(skip_all)]
    pub fn set_excerpts_for_path(
        &mut self,
        path: PathKey,
        buffer: Entity<Buffer>,
        ranges: impl IntoIterator<Item = Range<Point>>,
        context_line_count: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let buffer_snapshot = buffer.read(cx).snapshot();
        let ranges: Vec<_> = ranges.into_iter().collect();
        let excerpt_ranges = build_excerpt_ranges(ranges, context_line_count, &buffer_snapshot);

        let merged = Self::merge_excerpt_ranges(&excerpt_ranges);
        let (inserted, _path_key_index) =
            self.set_merged_excerpt_ranges_for_path(path, buffer, &buffer_snapshot, merged, cx);
        inserted
    }

    /// Like [`Self::set_excerpts_for_path`], but expands the provided ranges to cover any overlapping existing excerpts
    /// for the same buffer and path.
    ///
    /// Existing excerpts that do not overlap any of the provided ranges are discarded.
    pub fn update_excerpts_for_path(
        &mut self,
        path: PathKey,
        buffer: Entity<Buffer>,
        ranges: impl IntoIterator<Item = Range<Point>>,
        context_line_count: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let buffer_snapshot = buffer.read(cx).snapshot();
        let ranges: Vec<_> = ranges.into_iter().collect();
        let excerpt_ranges = build_excerpt_ranges(ranges, context_line_count, &buffer_snapshot);
        let merged = self.merge_new_with_existing_excerpt_ranges(
            &path,
            &buffer_snapshot,
            excerpt_ranges,
            cx,
        );

        let (inserted, _path_key_index) =
            self.set_merged_excerpt_ranges_for_path(path, buffer, &buffer_snapshot, merged, cx);
        inserted
    }

    pub fn merge_new_with_existing_excerpt_ranges(
        &self,
        path: &PathKey,
        buffer_snapshot: &BufferSnapshot,
        mut excerpt_ranges: Vec<ExcerptRange<Point>>,
        cx: &App,
    ) -> Vec<ExcerptRange<Point>> {
        let multibuffer_snapshot = self.snapshot(cx);

        if multibuffer_snapshot.path_for_buffer(buffer_snapshot.remote_id()) == Some(path) {
            excerpt_ranges.sort_by_key(|range| range.context.start);
            let mut combined_ranges = Vec::new();
            let mut new_ranges = excerpt_ranges.into_iter().peekable();
            for existing_range in
                multibuffer_snapshot.excerpts_for_buffer(buffer_snapshot.remote_id())
            {
                let existing_range = ExcerptRange {
                    context: existing_range.context.to_point(buffer_snapshot),
                    primary: existing_range.primary.to_point(buffer_snapshot),
                };
                while let Some(new_range) = new_ranges.peek()
                    && new_range.context.end < existing_range.context.start
                {
                    combined_ranges.push(new_range.clone());
                    new_ranges.next();
                }

                if let Some(new_range) = new_ranges.peek()
                    && new_range.context.start <= existing_range.context.end
                {
                    combined_ranges.push(existing_range)
                }
            }
            combined_ranges.extend(new_ranges);
            excerpt_ranges = combined_ranges;
        }

        excerpt_ranges.sort_by_key(|range| range.context.start);
        Self::merge_excerpt_ranges(&excerpt_ranges)
    }

    pub fn set_excerpt_ranges_for_path(
        &mut self,
        path: PathKey,
        buffer: Entity<Buffer>,
        buffer_snapshot: &BufferSnapshot,
        excerpt_ranges: Vec<ExcerptRange<Point>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let merged = Self::merge_excerpt_ranges(&excerpt_ranges);
        let (inserted, _path_key_index) =
            self.set_merged_excerpt_ranges_for_path(path, buffer, buffer_snapshot, merged, cx);
        inserted
    }

    pub fn set_anchored_excerpts_for_path(
        &self,
        path_key: PathKey,
        buffer: Entity<Buffer>,
        ranges: Vec<Range<text::Anchor>>,
        context_line_count: u32,
        cx: &Context<Self>,
    ) -> impl Future<Output = Vec<Range<Anchor>>> + use<> {
        let buffer_snapshot = buffer.read(cx).snapshot();
        let multi_buffer = cx.weak_entity();
        let mut app = cx.to_async();
        async move {
            let snapshot = buffer_snapshot.clone();
            let (ranges, merged_excerpt_ranges) = app
                .background_spawn(async move {
                    let point_ranges = ranges.iter().map(|range| range.to_point(&snapshot));
                    let excerpt_ranges =
                        build_excerpt_ranges(point_ranges, context_line_count, &snapshot);
                    let merged = Self::merge_excerpt_ranges(&excerpt_ranges);
                    (ranges, merged)
                })
                .await;

            multi_buffer
                .update(&mut app, move |multi_buffer, cx| {
                    let (_, path_key_index) = multi_buffer.set_merged_excerpt_ranges_for_path(
                        path_key,
                        buffer,
                        &buffer_snapshot,
                        merged_excerpt_ranges,
                        cx,
                    );
                    ranges
                        .into_iter()
                        .map(|range| Anchor::range_in_buffer(path_key_index, range))
                        .collect()
                })
                .ok()
                .unwrap_or_default()
        }
    }

    pub fn expand_excerpts(
        &mut self,
        anchors: impl IntoIterator<Item = Anchor>,
        line_count: u32,
        direction: ExpandExcerptDirection,
        cx: &mut Context<Self>,
    ) {
        if line_count == 0 {
            return;
        }

        let snapshot = self.snapshot(cx);
        let mut sorted_anchors = anchors
            .into_iter()
            .filter_map(|anchor| anchor.excerpt_anchor())
            .collect::<Vec<_>>();
        if sorted_anchors.is_empty() {
            return;
        }
        sorted_anchors.sort_by(|a, b| a.cmp(b, &snapshot));
        let buffers = sorted_anchors.into_iter().chunk_by(|anchor| anchor.path);
        let mut cursor = snapshot.excerpts.cursor::<ExcerptSummary>(());

        for (path_index, excerpt_anchors) in &buffers {
            let path = snapshot
                .path_keys
                .get_index(path_index.0 as usize)
                .expect("anchor from wrong multibuffer");

            let mut excerpt_anchors = excerpt_anchors.peekable();
            let mut ranges = Vec::new();

            cursor.seek_forward(path, Bias::Left);
            let Some((buffer, buffer_snapshot)) = cursor
                .item()
                .map(|excerpt| (excerpt.buffer(&self), excerpt.buffer_snapshot(&snapshot)))
            else {
                continue;
            };

            while let Some(excerpt) = cursor.item()
                && &excerpt.path_key == path
            {
                let mut range = ExcerptRange {
                    context: excerpt.range.context.to_point(buffer_snapshot),
                    primary: excerpt.range.primary.to_point(buffer_snapshot),
                };

                let mut needs_expand = false;
                while excerpt_anchors.peek().is_some_and(|anchor| {
                    excerpt
                        .range
                        .contains(&anchor.text_anchor(), buffer_snapshot)
                }) {
                    needs_expand = true;
                    excerpt_anchors.next();
                }

                if needs_expand {
                    match direction {
                        ExpandExcerptDirection::Up => {
                            range.context.start.row =
                                range.context.start.row.saturating_sub(line_count);
                            range.context.start.column = 0;
                        }
                        ExpandExcerptDirection::Down => {
                            range.context.end.row = (range.context.end.row + line_count)
                                .min(excerpt.buffer_snapshot(&snapshot).max_point().row);
                            range.context.end.column = excerpt
                                .buffer_snapshot(&snapshot)
                                .line_len(range.context.end.row);
                        }
                        ExpandExcerptDirection::UpAndDown => {
                            range.context.start.row =
                                range.context.start.row.saturating_sub(line_count);
                            range.context.start.column = 0;
                            range.context.end.row = (range.context.end.row + line_count)
                                .min(excerpt.buffer_snapshot(&snapshot).max_point().row);
                            range.context.end.column = excerpt
                                .buffer_snapshot(&snapshot)
                                .line_len(range.context.end.row);
                        }
                    }
                }

                ranges.push(range);
                cursor.next();
            }

            ranges.sort_by_key(|r| r.context.start);

            self.set_excerpt_ranges_for_path(path.clone(), buffer, buffer_snapshot, ranges, cx);
        }
    }
}

mod excerpt_updates;

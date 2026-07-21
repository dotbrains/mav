use super::*;

/// A naive implementation of a multi-buffer that does not maintain
/// any derived state, used for comparison in a randomized test.
#[derive(Default)]
pub(super) struct ReferenceMultibuffer {
    pub(super) excerpts: Vec<ReferenceExcerpt>,
    pub(super) diffs: HashMap<BufferId, Entity<BufferDiff>>,
    pub(super) inverted_diffs: HashMap<BufferId, (Entity<BufferDiff>, Entity<language::Buffer>)>,
    pub(super) expanded_diff_hunks_by_buffer: HashMap<BufferId, Vec<text::Anchor>>,
}

#[derive(Clone, Debug)]
pub(super) struct ReferenceExcerpt {
    pub(super) path_key: PathKey,
    pub(super) path_key_index: PathKeyIndex,
    pub(super) buffer: Entity<Buffer>,
    pub(super) range: Range<text::Anchor>,
}

#[derive(Clone, Debug)]
pub(super) struct ReferenceRegion {
    pub(super) buffer_id: Option<BufferId>,
    pub(super) range: Range<usize>,
    pub(super) buffer_range: Range<Point>,
    // if this is a deleted hunk, the main buffer anchor to which the deleted content is attached
    pub(super) deleted_hunk_anchor: Option<text::Anchor>,
    pub(super) status: Option<DiffHunkStatus>,
    pub(super) excerpt: Option<ReferenceExcerpt>,
}

impl ReferenceMultibuffer {
    pub(super) fn expand_excerpts(
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

    pub(super) fn set_excerpts(
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

    pub(super) fn expand_diff_hunks(
        &mut self,
        path_key: PathKey,
        range: Range<text::Anchor>,
        cx: &App,
    ) {
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
}

impl ReferenceMultibuffer {
    pub(super) fn diffs_updated(&mut self, cx: &mut App) {
        let buffer_ids = self.diffs.keys().copied().collect::<Vec<_>>();
        for buffer_id in buffer_ids {
            self.update_expanded_diff_hunks_for_buffer(buffer_id, cx);
        }
    }

    pub(super) fn add_diff(&mut self, diff: Entity<BufferDiff>, cx: &mut App) {
        let buffer_id = diff.read(cx).buffer_id;
        self.diffs.insert(buffer_id, diff);
    }

    pub(super) fn add_inverted_diff(
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

    pub(super) fn anchor_to_offset(&self, anchor: &Anchor, cx: &App) -> Option<MultiBufferOffset> {
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

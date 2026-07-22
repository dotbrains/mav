use std::{cmp::Ordering, ops::Range};
use text::{Anchor, BufferId, BufferSnapshot};

use super::types::SyntaxLayerSummary;
use super::types::*;

impl PartialEq for ParseStep {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

impl Eq for ParseStep {}

impl PartialOrd for ParseStep {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ParseStep {
    fn cmp(&self, other: &Self) -> Ordering {
        let range_a = self.range();
        let range_b = other.range();
        Ord::cmp(&other.depth, &self.depth)
            .then_with(|| Ord::cmp(&range_b.start, &range_a.start))
            .then_with(|| Ord::cmp(&range_a.end, &range_b.end))
            .then_with(|| other.language.id().cmp(&self.language.id()))
    }
}

impl ParseStep {
    fn range(&self) -> Range<usize> {
        if let ParseMode::Combined {
            parent_layer_range, ..
        } = &self.mode
        {
            parent_layer_range.clone()
        } else {
            let start = self.included_ranges.first().map_or(0, |r| r.start_byte);
            let end = self.included_ranges.last().map_or(0, |r| r.end_byte);
            start..end
        }
    }
}

impl ChangedRegion {
    fn cmp(&self, other: &Self, buffer: &BufferSnapshot) -> Ordering {
        let range_a = &self.range;
        let range_b = &other.range;
        Ord::cmp(&self.depth, &other.depth)
            .then_with(|| range_a.start.cmp(&range_b.start, buffer))
            .then_with(|| range_b.end.cmp(&range_a.end, buffer))
    }
}

impl ChangeRegionSet {
    pub(super) fn start_position(&self, buffer_id: BufferId) -> ChangeStartPosition {
        self.0.first().map_or(
            ChangeStartPosition {
                depth: usize::MAX,
                position: Anchor::max_for_buffer(buffer_id),
            },
            |region| ChangeStartPosition {
                depth: region.depth,
                position: region.range.start,
            },
        )
    }

    pub(super) fn intersects(&self, layer: &SyntaxLayerEntry, text: &BufferSnapshot) -> bool {
        for region in &self.0 {
            if region.depth < layer.depth {
                continue;
            }
            if region.depth > layer.depth {
                break;
            }
            if region.range.end.cmp(&layer.range.start, text).is_le() {
                continue;
            }
            if region.range.start.cmp(&layer.range.end, text).is_ge() {
                break;
            }
            return true;
        }
        false
    }

    pub(super) fn insert(&mut self, region: ChangedRegion, text: &BufferSnapshot) {
        if let Err(ix) = self.0.binary_search_by(|probe| probe.cmp(&region, text)) {
            self.0.insert(ix, region);
        }
    }

    pub(super) fn prune(&mut self, summary: SyntaxLayerSummary, text: &BufferSnapshot) -> bool {
        let prev_len = self.0.len();
        self.0.retain(|region| {
            region.depth > summary.max_depth
                || (region.depth == summary.max_depth
                    && region
                        .range
                        .end
                        .cmp(&summary.last_layer_range.start, text)
                        .is_gt())
        });
        self.0.len() < prev_len
    }
}

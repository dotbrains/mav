use super::*;

#[derive(Debug)]
pub struct CompanionExcerptPatch {
    pub patch: Patch<MultiBufferPoint>,
    pub edited_range: Range<MultiBufferPoint>,
    pub source_excerpt_range: Range<MultiBufferPoint>,
    pub target_excerpt_range: Range<MultiBufferPoint>,
}

pub(crate) struct Companion {
    rhs_display_map_id: EntityId,
    rhs_custom_block_to_balancing_block: RefCell<HashMap<CustomBlockId, CustomBlockId>>,
    lhs_custom_block_to_balancing_block: RefCell<HashMap<CustomBlockId, CustomBlockId>>,
}

impl Companion {
    pub(crate) fn new(rhs_display_map_id: EntityId) -> Self {
        Self {
            rhs_display_map_id,
            rhs_custom_block_to_balancing_block: Default::default(),
            lhs_custom_block_to_balancing_block: Default::default(),
        }
    }

    pub(crate) fn is_rhs(&self, display_map_id: EntityId) -> bool {
        self.rhs_display_map_id == display_map_id
    }

    pub(crate) fn custom_block_to_balancing_block(
        &self,
        display_map_id: EntityId,
    ) -> &RefCell<HashMap<CustomBlockId, CustomBlockId>> {
        if self.is_rhs(display_map_id) {
            &self.rhs_custom_block_to_balancing_block
        } else {
            &self.lhs_custom_block_to_balancing_block
        }
    }

    pub(crate) fn convert_rows_to_companion(
        &self,
        display_map_id: EntityId,
        companion_snapshot: &MultiBufferSnapshot,
        our_snapshot: &MultiBufferSnapshot,
        bounds: Range<MultiBufferPoint>,
    ) -> Vec<CompanionExcerptPatch> {
        if self.is_rhs(display_map_id) {
            crate::split::patches_for_rhs_range(companion_snapshot, our_snapshot, bounds)
        } else {
            crate::split::patches_for_lhs_range(companion_snapshot, our_snapshot, bounds)
        }
    }

    pub(crate) fn convert_point_from_companion(
        &self,
        display_map_id: EntityId,
        our_snapshot: &MultiBufferSnapshot,
        companion_snapshot: &MultiBufferSnapshot,
        point: MultiBufferPoint,
    ) -> Range<MultiBufferPoint> {
        let patches = if self.is_rhs(display_map_id) {
            crate::split::patches_for_lhs_range(our_snapshot, companion_snapshot, point..point)
        } else {
            crate::split::patches_for_rhs_range(our_snapshot, companion_snapshot, point..point)
        };

        let Some(excerpt) = patches.into_iter().next() else {
            if cfg!(any(test, debug_assertions)) {
                assert!(
                    our_snapshot.max_point() == Point::zero(),
                    "`patches_for_*_in_range` is only allowed to return an empty vec if the multibuffer is empty"
                );
            }
            return Point::zero()..our_snapshot.max_point();
        };
        excerpt.patch.edit_for_old_position(point).new
    }

    pub(crate) fn convert_point_to_companion(
        &self,
        display_map_id: EntityId,
        our_snapshot: &MultiBufferSnapshot,
        companion_snapshot: &MultiBufferSnapshot,
        point: MultiBufferPoint,
    ) -> Range<MultiBufferPoint> {
        let patches = if self.is_rhs(display_map_id) {
            crate::split::patches_for_rhs_range(companion_snapshot, our_snapshot, point..point)
        } else {
            crate::split::patches_for_lhs_range(companion_snapshot, our_snapshot, point..point)
        };

        let Some(excerpt) = patches.into_iter().next() else {
            return Point::zero()..companion_snapshot.max_point();
        };
        excerpt.patch.edit_for_old_position(point).new
    }
}

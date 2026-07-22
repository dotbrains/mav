use super::*;

impl Deref for BlockMapReader<'_> {
    type Target = BlockSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl DerefMut for BlockMapReader<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.snapshot
    }
}

impl BlockMapReader<'_> {
    #[ztracing::instrument(skip_all)]
    pub fn row_for_block(&self, block_id: CustomBlockId) -> Option<BlockRow> {
        let block = self.blocks.iter().find(|block| block.id == block_id)?;
        let buffer_row = block
            .start()
            .to_point(self.wrap_snapshot.buffer_snapshot())
            .row;
        let wrap_row = self
            .wrap_snapshot
            .make_wrap_point(Point::new(buffer_row, 0), Bias::Left)
            .row();
        let start_wrap_row = self
            .wrap_snapshot
            .prev_row_boundary(WrapPoint::new(wrap_row, 0));
        let end_wrap_row = self
            .wrap_snapshot
            .next_row_boundary(WrapPoint::new(wrap_row, 0))
            .unwrap_or(self.wrap_snapshot.max_point().row() + WrapRow(1));

        let mut cursor = self.transforms.cursor::<Dimensions<WrapRow, BlockRow>>(());
        cursor.seek(&start_wrap_row, Bias::Left);
        while let Some(transform) = cursor.item() {
            if cursor.start().0 > end_wrap_row {
                break;
            }

            if let Some(BlockId::Custom(id)) = transform.block.as_ref().map(|block| block.id())
                && id == block_id
            {
                return Some(cursor.start().1);
            }
            cursor.next();
        }

        None
    }
}

pub(crate) fn balancing_block(
    my_block: &BlockProperties<Anchor>,
    my_snapshot: &MultiBufferSnapshot,
    their_snapshot: &MultiBufferSnapshot,
    my_display_map_id: EntityId,
    companion: &Companion,
) -> Option<BlockProperties<Anchor>> {
    let my_anchor = my_block.placement.start();
    let my_point = my_anchor.to_point(&my_snapshot);
    let their_range = companion.convert_point_to_companion(
        my_display_map_id,
        my_snapshot,
        their_snapshot,
        my_point,
    );
    let their_anchor = their_snapshot.anchor_at(their_range.start, my_anchor.bias());
    let their_placement = match my_block.placement {
        BlockPlacement::Above(_) => BlockPlacement::Above(their_anchor),
        BlockPlacement::Below(_) => {
            if their_range.is_empty() {
                BlockPlacement::Above(their_anchor)
            } else {
                BlockPlacement::Below(their_anchor)
            }
        }
        // Not supported for balancing
        BlockPlacement::Near(_) | BlockPlacement::Replace(_) => return None,
    };
    Some(BlockProperties {
        placement: their_placement,
        height: my_block.height,
        style: BlockStyle::Spacer,
        render: Arc::new(move |cx| {
            crate::EditorElement::render_spacer_block(
                cx.block_id,
                cx.height,
                cx.line_height,
                cx.indent_guide_padding,
                cx.window,
                cx.app,
            )
        }),
        priority: my_block.priority,
    })
}

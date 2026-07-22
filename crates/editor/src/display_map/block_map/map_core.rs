use super::*;

impl BlockMap {
    #[ztracing::instrument(skip_all)]
    pub fn new(
        wrap_snapshot: WrapSnapshot,
        buffer_header_height: u32,
        excerpt_header_height: u32,
    ) -> Self {
        let row_count = wrap_snapshot.max_point().row() + WrapRow(1);
        let mut transforms = SumTree::default();
        push_isomorphic(&mut transforms, row_count - WrapRow(0), &wrap_snapshot);
        let map = Self {
            next_block_id: AtomicUsize::new(0),
            custom_blocks: Vec::new(),
            custom_blocks_by_id: TreeMap::default(),
            folded_buffers: HashSet::default(),
            buffers_with_disabled_headers: HashSet::default(),
            transforms: RefCell::new(transforms),
            wrap_snapshot: RefCell::new(wrap_snapshot.clone()),
            buffer_header_height,
            excerpt_header_height,
            deferred_edits: Cell::default(),
        };
        map.sync(
            &wrap_snapshot,
            Patch::new(vec![Edit {
                old: WrapRow(0)..row_count,
                new: WrapRow(0)..row_count,
            }]),
            None,
        );
        map
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn read(
        &self,
        wrap_snapshot: WrapSnapshot,
        edits: WrapPatch,
        companion_view: Option<CompanionView>,
    ) -> BlockMapReader<'_> {
        self.sync(&wrap_snapshot, edits, companion_view);
        *self.wrap_snapshot.borrow_mut() = wrap_snapshot.clone();
        BlockMapReader {
            blocks: &self.custom_blocks,
            snapshot: BlockSnapshot {
                wrap_snapshot,
                transforms: self.transforms.borrow().clone(),
                custom_blocks_by_id: self.custom_blocks_by_id.clone(),
                buffer_header_height: self.buffer_header_height,
                excerpt_header_height: self.excerpt_header_height,
                buffers_with_disabled_headers: self.buffers_with_disabled_headers.clone(),
            },
        }
    }

    #[ztracing::instrument(skip_all)]
    pub(crate) fn write<'a>(
        &'a mut self,
        wrap_snapshot: WrapSnapshot,
        edits: WrapPatch,
        companion_view: Option<CompanionViewMut<'a>>,
    ) -> BlockMapWriter<'a> {
        self.sync(
            &wrap_snapshot,
            edits.clone(),
            companion_view.as_ref().map(CompanionView::from),
        );
        *self.wrap_snapshot.borrow_mut() = wrap_snapshot.clone();
        let companion = if let Some(companion_view) = companion_view {
            companion_view.companion_block_map.sync(
                companion_view.companion_wrap_snapshot,
                companion_view.companion_wrap_edits.clone(),
                Some(CompanionView::new(
                    companion_view.companion_display_map_id,
                    &wrap_snapshot,
                    &edits,
                    companion_view.companion,
                )),
            );
            *companion_view
                .companion_block_map
                .wrap_snapshot
                .borrow_mut() = companion_view.companion_wrap_snapshot.clone();
            Some(BlockMapWriterCompanion {
                display_map_id: companion_view.display_map_id,
                companion_wrap_snapshot: companion_view.companion_wrap_snapshot.clone(),
                companion: companion_view.companion,
                inverse: Some(BlockMapInverseWriter {
                    companion_multibuffer: companion_view.companion_multibuffer,
                    companion_writer: Box::new(BlockMapWriter {
                        block_map: companion_view.companion_block_map,
                        companion: Some(BlockMapWriterCompanion {
                            display_map_id: companion_view.companion_display_map_id,
                            companion_wrap_snapshot: wrap_snapshot,
                            companion: companion_view.companion,
                            inverse: None,
                        }),
                    }),
                }),
            })
        } else {
            None
        };
        BlockMapWriter {
            block_map: self,
            companion,
        }
    }

    // Warning: doesn't sync the block map, use advisedly
    pub(crate) fn insert_block_raw(
        &mut self,
        block: BlockProperties<Anchor>,
        buffer: &MultiBufferSnapshot,
    ) -> CustomBlockId {
        let id = CustomBlockId(self.next_block_id.fetch_add(1, SeqCst));
        let block_ix = match self
            .custom_blocks
            .binary_search_by(|probe| probe.placement.cmp(&block.placement, &buffer))
        {
            Ok(ix) | Err(ix) => ix,
        };
        let new_block = Arc::new(CustomBlock {
            id,
            placement: block.placement.clone(),
            height: block.height,
            style: block.style,
            render: Arc::new(Mutex::new(block.render.clone())),
            priority: block.priority,
        });
        self.custom_blocks.insert(block_ix, new_block.clone());
        self.custom_blocks_by_id.insert(id, new_block);
        id
    }

    // Warning: doesn't sync the block map, use advisedly
    pub(crate) fn retain_blocks_raw(&mut self, pred: &mut dyn FnMut(&Arc<CustomBlock>) -> bool) {
        let mut ids_to_remove = HashSet::default();
        self.custom_blocks.retain(|block| {
            let keep = pred(block);
            if !keep {
                ids_to_remove.insert(block.id);
            }
            keep
        });
        self.custom_blocks_by_id
            .retain(|id, _| !ids_to_remove.contains(id));
    }

    // Warning: doesn't sync the block map, use advisedly
    pub(crate) fn blocks_raw(&self) -> impl Iterator<Item = &Arc<CustomBlock>> {
        self.custom_blocks.iter()
    }
}

use super::*;

impl Pane {
    pub fn items_len(&self) -> usize {
        self.items.len()
    }

    pub fn items(&self) -> impl DoubleEndedIterator<Item = &Box<dyn ItemHandle>> {
        self.items.iter()
    }

    pub fn items_of_type<T: Render>(&self) -> impl '_ + Iterator<Item = Entity<T>> {
        self.items
            .iter()
            .filter_map(|item| item.to_any_view().downcast().ok())
    }

    pub fn active_item(&self) -> Option<Box<dyn ItemHandle>> {
        self.items.get(self.active_item_index).cloned()
    }

    pub(super) fn active_item_id(&self) -> EntityId {
        self.items[self.active_item_index].item_id()
    }

    pub fn pixel_position_of_cursor(&self, cx: &App) -> Option<Point<Pixels>> {
        self.items
            .get(self.active_item_index)?
            .pixel_position_of_cursor(cx)
    }

    pub fn item_for_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &App,
    ) -> Option<Box<dyn ItemHandle>> {
        self.items.iter().find_map(|item| {
            if item.buffer_kind(cx) == ItemBufferKind::Singleton
                && (item.project_entry_ids(cx).as_slice() == [entry_id])
            {
                Some(item.boxed_clone())
            } else {
                None
            }
        })
    }

    pub fn item_for_path(
        &self,
        project_path: ProjectPath,
        cx: &App,
    ) -> Option<Box<dyn ItemHandle>> {
        self.items.iter().find_map(move |item| {
            if item.buffer_kind(cx) == ItemBufferKind::Singleton
                && (item.project_path(cx).as_slice() == [project_path.clone()])
            {
                Some(item.boxed_clone())
            } else {
                None
            }
        })
    }

    pub fn index_for_item(&self, item: &dyn ItemHandle) -> Option<usize> {
        self.index_for_item_id(item.item_id())
    }

    pub(super) fn index_for_item_id(&self, item_id: EntityId) -> Option<usize> {
        self.items.iter().position(|i| i.item_id() == item_id)
    }

    pub fn item_for_index(&self, ix: usize) -> Option<&dyn ItemHandle> {
        self.items.get(ix).map(|i| i.as_ref())
    }
}

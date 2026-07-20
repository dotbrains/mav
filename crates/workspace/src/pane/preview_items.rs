use super::*;

impl Pane {
    pub fn preview_item_id(&self) -> Option<EntityId> {
        self.preview_item_id
    }

    pub fn preview_item(&self) -> Option<Box<dyn ItemHandle>> {
        self.preview_item_id
            .and_then(|id| self.items.iter().find(|item| item.item_id() == id))
            .cloned()
    }

    pub fn preview_item_idx(&self) -> Option<usize> {
        if let Some(preview_item_id) = self.preview_item_id {
            self.items
                .iter()
                .position(|item| item.item_id() == preview_item_id)
        } else {
            None
        }
    }

    pub fn is_active_preview_item(&self, item_id: EntityId) -> bool {
        self.preview_item_id == Some(item_id)
    }

    /// Promotes the item with the given ID to not be a preview item.
    /// This does nothing if it wasn't already a preview item.
    pub fn unpreview_item_if_preview(&mut self, item_id: EntityId) {
        if self.is_active_preview_item(item_id) {
            self.preview_item_id = None;
            self.nav_history.0.lock().preview_item_id = None;
        }
    }

    /// Marks the item with the given ID as the preview item.
    /// This will be ignored if the global setting `preview_tabs` is disabled.
    ///
    /// The old preview item (if there was one) is closed and its index is returned.
    pub fn replace_preview_item_id(
        &mut self,
        item_id: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let idx = self.close_current_preview_item(window, cx);
        self.set_preview_item_id(Some(item_id), cx);
        idx
    }

    /// Marks the item with the given ID as the preview item.
    /// This will be ignored if the global setting `preview_tabs` is disabled.
    ///
    /// This is a low-level method. Prefer `unpreview_item_if_preview()` or `set_new_preview_item()`.
    pub(crate) fn set_preview_item_id(&mut self, item_id: Option<EntityId>, cx: &App) {
        if item_id.is_none() || PreviewTabsSettings::get_global(cx).enabled {
            self.preview_item_id = item_id;
            self.nav_history.0.lock().preview_item_id = item_id;
        }
    }
}

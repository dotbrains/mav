use super::*;

impl Pane {
    pub fn close_active_item(
        &mut self,
        action: &CloseActiveItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if !self.is_tabbed() {
            return Task::ready(Ok(()));
        }

        if self.items.is_empty() {
            // Close the window when there's no active items to close, if configured
            if WorkspaceSettings::get_global(cx)
                .when_closing_with_no_tabs
                .should_close()
            {
                window.dispatch_action(Box::new(CloseWindow), cx);
            }

            return Task::ready(Ok(()));
        }
        if self.is_tab_pinned(self.active_item_index) && !action.close_pinned {
            // Activate any non-pinned tab in same pane
            let non_pinned_tab_index = self
                .items()
                .enumerate()
                .find(|(index, _item)| !self.is_tab_pinned(*index))
                .map(|(index, _item)| index);
            if let Some(index) = non_pinned_tab_index {
                self.activate_item(index, false, false, window, cx);
                return Task::ready(Ok(()));
            }

            // Activate any non-pinned tab in different pane
            let current_pane = cx.entity();
            self.workspace
                .update(cx, |workspace, cx| {
                    let panes = workspace.center.panes();
                    let pane_with_unpinned_tab = panes.iter().find(|pane| {
                        if **pane == &current_pane {
                            return false;
                        }
                        pane.read(cx).has_unpinned_tabs()
                    });
                    if let Some(pane) = pane_with_unpinned_tab {
                        pane.update(cx, |pane, cx| pane.activate_unpinned_tab(window, cx));
                    }
                })
                .ok();

            return Task::ready(Ok(()));
        };

        let active_item_id = self.active_item_id();

        self.close_item_by_id(
            active_item_id,
            action.save_intent.unwrap_or(SaveIntent::Close),
            window,
            cx,
        )
    }

    pub fn close_item_by_id(
        &mut self,
        item_id_to_close: EntityId,
        save_intent: SaveIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.close_items(window, cx, save_intent, &move |view_id| {
            view_id == item_id_to_close
        })
    }

    pub fn close_items_for_project_path(
        &mut self,
        project_path: &ProjectPath,
        save_intent: SaveIntent,
        close_pinned: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let pinned_item_ids = self.pinned_item_ids();
        let matching_item_ids: Vec<_> = self
            .items()
            .filter(|item| item.project_path(cx).as_ref() == Some(project_path))
            .map(|item| item.item_id())
            .collect();
        self.close_items(window, cx, save_intent, &move |item_id| {
            matching_item_ids.contains(&item_id)
                && (close_pinned || !pinned_item_ids.contains(&item_id))
        })
    }

    pub fn close_other_items(
        &mut self,
        action: &CloseOtherItems,
        target_item_id: Option<EntityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.items.is_empty() {
            return Task::ready(Ok(()));
        }

        let active_item_id = match target_item_id {
            Some(result) => result,
            None => self.active_item_id(),
        };

        self.unpreview_item_if_preview(active_item_id);

        let pinned_item_ids = self.pinned_item_ids();

        self.close_items(
            window,
            cx,
            action.save_intent.unwrap_or(SaveIntent::Close),
            &move |item_id| {
                item_id != active_item_id
                    && (action.close_pinned || !pinned_item_ids.contains(&item_id))
            },
        )
    }

    pub fn close_multibuffer_items(
        &mut self,
        action: &CloseMultibufferItems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.items.is_empty() {
            return Task::ready(Ok(()));
        }

        let pinned_item_ids = self.pinned_item_ids();
        let multibuffer_items = self.multibuffer_item_ids(cx);

        self.close_items(
            window,
            cx,
            action.save_intent.unwrap_or(SaveIntent::Close),
            &move |item_id| {
                (action.close_pinned || !pinned_item_ids.contains(&item_id))
                    && multibuffer_items.contains(&item_id)
            },
        )
    }

    pub fn close_clean_items(
        &mut self,
        action: &CloseCleanItems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.items.is_empty() {
            return Task::ready(Ok(()));
        }

        let clean_item_ids = self.clean_item_ids(cx);
        let pinned_item_ids = self.pinned_item_ids();

        self.close_items(window, cx, SaveIntent::Close, &move |item_id| {
            clean_item_ids.contains(&item_id)
                && (action.close_pinned || !pinned_item_ids.contains(&item_id))
        })
    }

    pub fn close_items_to_the_left_by_id(
        &mut self,
        item_id: Option<EntityId>,
        action: &CloseItemsToTheLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.close_items_to_the_side_by_id(item_id, Side::Left, action.close_pinned, window, cx)
    }

    pub fn close_items_to_the_right_by_id(
        &mut self,
        item_id: Option<EntityId>,
        action: &CloseItemsToTheRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.close_items_to_the_side_by_id(item_id, Side::Right, action.close_pinned, window, cx)
    }

    pub(super) fn close_items_to_the_side_by_id(
        &mut self,
        item_id: Option<EntityId>,
        side: Side,
        close_pinned: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.items.is_empty() {
            return Task::ready(Ok(()));
        }

        let item_id = item_id.unwrap_or_else(|| self.active_item_id());
        let to_the_side_item_ids = self.to_the_side_item_ids(item_id, side);
        let pinned_item_ids = self.pinned_item_ids();

        self.close_items(window, cx, SaveIntent::Close, &move |item_id| {
            to_the_side_item_ids.contains(&item_id)
                && (close_pinned || !pinned_item_ids.contains(&item_id))
        })
    }

    pub fn close_all_items(
        &mut self,
        action: &CloseAllItems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.items.is_empty() {
            return Task::ready(Ok(()));
        }

        let pinned_item_ids = self.pinned_item_ids();

        self.close_items(
            window,
            cx,
            action.save_intent.unwrap_or(SaveIntent::Close),
            &|item_id| action.close_pinned || !pinned_item_ids.contains(&item_id),
        )
    }
}

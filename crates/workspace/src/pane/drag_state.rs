use super::*;

impl Pane {
    pub(super) fn clear_drag_drop_target(&mut self, cx: &mut Context<Self>) {
        if self.drag_split_direction.take().is_some()
            || mem::take(&mut self.drag_swap_target)
            || mem::take(&mut self.drag_tab_target)
            || self.drag_tab_insertion_target.take().is_some()
        {
            cx.notify();
        }
    }

    pub(super) fn clear_body_drag_drop_target(&mut self, cx: &mut Context<Self>) {
        if self.drag_split_direction.take().is_some()
            || mem::take(&mut self.drag_swap_target)
            || mem::take(&mut self.drag_tab_target)
        {
            cx.notify();
        }
    }

    fn is_over_tab_bar_above_body<T>(event: &DragMoveEvent<T>, cx: &App) -> bool {
        let position = event.event.position;
        let tab_bar_height = Tab::container_height(cx) * 2.;

        position.x >= event.bounds.left()
            && position.x <= event.bounds.right()
            && position.y < event.bounds.top()
            && position.y >= event.bounds.top() - tab_bar_height
    }

    fn set_tab_insertion_target(
        &mut self,
        target: Option<TabInsertionTarget>,
        cx: &mut Context<Self>,
    ) {
        let changed = self.drag_split_direction.take().is_some()
            || mem::take(&mut self.drag_swap_target)
            || mem::take(&mut self.drag_tab_target)
            || self.drag_tab_insertion_target != target;

        self.drag_tab_insertion_target = target;

        if changed {
            cx.notify();
        }
    }

    fn set_drag_drop_target(
        &mut self,
        split_direction: Option<SplitDirection>,
        swap: bool,
        tab_target: bool,
        cx: &mut Context<Self>,
    ) {
        if self.drag_split_direction != split_direction
            || self.drag_swap_target != swap
            || self.drag_tab_target != tab_target
            || self.drag_tab_insertion_target.is_some()
        {
            self.drag_split_direction = split_direction;
            self.drag_swap_target = swap;
            self.drag_tab_target = tab_target;
            self.drag_tab_insertion_target = None;
            cx.notify();
        }
    }

    pub(super) fn take_drag_split_direction(&mut self) -> Option<SplitDirection> {
        self.drag_swap_target = false;
        self.drag_tab_target = false;
        self.drag_tab_insertion_target = None;
        self.drag_split_direction.take()
    }

    fn can_drop_as_tab_target(target_pane: &Entity<Pane>, dragged_item: &dyn Any) -> bool {
        if let Some(dragged_tab) = dragged_item.downcast_ref::<DraggedTab>() {
            return dragged_tab.pane != *target_pane;
        }

        dragged_item
            .downcast_ref::<DraggedSelection>()
            .is_some_and(|selection| selection.active_selection_is_file)
    }

    pub(super) fn handle_dragged_tab_over_tab(
        &mut self,
        ix: usize,
        event: &DragMoveEvent<DraggedTab>,
        cx: &mut Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            return;
        }

        let dragged_tab = event.drag(cx);
        let target = if dragged_tab.pane != cx.entity() || ix < dragged_tab.ix {
            Some(TabInsertionTarget::Tab {
                ix,
                side: TabInsertionSide::Left,
            })
        } else if ix > dragged_tab.ix {
            Some(TabInsertionTarget::Tab {
                ix,
                side: TabInsertionSide::Right,
            })
        } else {
            None
        };

        self.set_tab_insertion_target(target, cx);
    }

    pub(super) fn handle_dragged_selection_over_tab(
        &mut self,
        ix: usize,
        event: &DragMoveEvent<DraggedSelection>,
        cx: &mut Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            return;
        }

        let target = event
            .drag(cx)
            .active_selection_is_file
            .then_some(TabInsertionTarget::Tab {
                ix,
                side: TabInsertionSide::Left,
            });

        self.set_tab_insertion_target(target, cx);
    }

    pub(super) fn handle_dragged_tab_over_tab_bar_end(
        &mut self,
        target: TabInsertionTarget,
        event: &DragMoveEvent<DraggedTab>,
        cx: &mut Context<Self>,
    ) {
        if event.bounds.contains(&event.event.position) {
            self.set_tab_insertion_target(Some(target), cx);
        }
    }

    pub(super) fn handle_dragged_selection_over_tab_bar_end(
        &mut self,
        target: TabInsertionTarget,
        event: &DragMoveEvent<DraggedSelection>,
        cx: &mut Context<Self>,
    ) {
        if !event.bounds.contains(&event.event.position) {
            return;
        }

        let target = event.drag(cx).active_selection_is_file.then_some(target);
        self.set_tab_insertion_target(target, cx);
    }

    pub(super) fn handle_drag_move<T: 'static>(
        &mut self,
        event: &DragMoveEvent<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_pane = cx.entity();
        if !event.bounds.contains(&event.event.position) {
            if Self::is_over_tab_bar_above_body(event, cx) {
                self.clear_body_drag_drop_target(cx);
            } else {
                self.clear_drag_drop_target(cx);
            }
            return;
        }

        if !Self::can_drop_on_body_target(
            &target_pane,
            self.items.len(),
            self.pane_kind,
            event.dragged_item(),
        ) {
            self.clear_drag_drop_target(cx);
            return;
        }

        let can_split_predicate = self.can_split_predicate.take();
        let can_split = match &can_split_predicate {
            Some(can_split_predicate) => {
                can_split_predicate(self, event.dragged_item(), window, cx)
            }
            None => false,
        };
        self.can_split_predicate = can_split_predicate;
        if !can_split {
            self.clear_drag_drop_target(cx);
            return;
        }

        let size = event.bounds.size;
        let horizontal_edge = (size.width * 0.25).max(px(80.)).min(size.width * 0.45);
        let vertical_edge = (size.height * 0.25).max(px(80.)).min(size.height * 0.45);

        let relative_cursor = Point::new(
            event.event.position.x - event.bounds.left(),
            event.event.position.y - event.bounds.top(),
        );

        let split_direction = if relative_cursor.x < horizontal_edge {
            Some(SplitDirection::Left)
        } else if relative_cursor.x > size.width - horizontal_edge {
            Some(SplitDirection::Right)
        } else if relative_cursor.y < vertical_edge {
            Some(SplitDirection::Up)
        } else if relative_cursor.y > size.height - vertical_edge {
            Some(SplitDirection::Down)
        } else {
            None
        };

        self.set_drag_drop_target(
            split_direction,
            split_direction.is_none() && event.dragged_item().is::<DraggedPane>(),
            split_direction.is_none()
                && Self::can_drop_as_tab_target(&target_pane, event.dragged_item())
                && self.is_tabbed(),
            cx,
        );
    }

    pub(super) fn can_drop_on_body_target(
        target_pane: &Entity<Pane>,
        target_items_len: usize,
        target_pane_kind: PaneKind,
        dragged_item: &dyn Any,
    ) -> bool {
        if let Some(dragged_pane) = dragged_item.downcast_ref::<DraggedPane>() {
            return dragged_pane.pane != *target_pane;
        }

        if let Some(dragged_tab) = dragged_item.downcast_ref::<DraggedTab>() {
            return dragged_tab.pane != *target_pane || target_items_len > 1;
        }

        if let Some(dragged_selection) = dragged_item.downcast_ref::<DraggedSelection>()
            && let Some(source_pane) = dragged_selection
                .source_pane
                .as_ref()
                .and_then(|source_pane| source_pane.upgrade())
        {
            if !dragged_selection.active_selection_is_file {
                return false;
            }
            return source_pane != *target_pane || target_pane_kind != PaneKind::Project;
        }

        if let Some(dragged_selection) = dragged_item.downcast_ref::<DraggedSelection>() {
            return dragged_selection.active_selection_is_file;
        }

        true
    }

    pub(super) fn can_drop_on_body(
        &self,
        dragged_item: &dyn Any,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !Self::can_drop_on_body_target(
            &cx.entity(),
            self.items.len(),
            self.pane_kind,
            dragged_item,
        ) {
            return false;
        }

        self.can_drop_predicate
            .as_ref()
            .is_none_or(|predicate| predicate(dragged_item, window, cx))
    }
}

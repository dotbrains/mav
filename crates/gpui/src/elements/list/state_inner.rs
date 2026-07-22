use super::*;

mod layout;

impl StateInner {
    /// Re-anchor a pending scroll adjustment from a remeasure onto a newly set
    /// scroll position, so it clamps to the remeasured item's new height on
    /// the next layout instead of reverting the scroll.
    pub(super) fn rebase_pending_scroll(&mut self, scroll_top: ListOffset) {
        let Some(pending) = self.pending_scroll.take() else {
            return;
        };
        if scroll_top.item_ix >= self.items.summary().count {
            return;
        }

        self.pending_scroll = match pending {
            PendingScroll::Absolute { .. } => Some(PendingScroll::Absolute {
                item_ix: scroll_top.item_ix,
                offset: scroll_top.offset_in_item,
            }),
            PendingScroll::Proportional(_) => {
                let mut cursor = self.items.cursor::<Count>(());
                cursor.seek(&Count(scroll_top.item_ix), Bias::Right);
                cursor
                    .item()
                    .and_then(|item| item.size_hint())
                    .filter(|size| size.height.0 > 0.0)
                    .map(|size| {
                        PendingScroll::Proportional(PendingScrollFraction {
                            item_ix: scroll_top.item_ix,
                            fraction: (scroll_top.offset_in_item.0 / size.height.0).clamp(0.0, 1.0),
                        })
                    })
            }
        };
    }

    pub(super) fn max_scroll_offset(&self) -> Pixels {
        let bounds = self.last_layout_bounds.unwrap_or_default();
        let height = self
            .scrollbar_drag_start_height
            .unwrap_or_else(|| self.items.summary().height);
        (height - bounds.size.height).max(px(0.))
    }

    pub(super) fn visible_range(
        items: &SumTree<ListItem>,
        height: Pixels,
        scroll_top: &ListOffset,
    ) -> Range<usize> {
        let mut cursor = items.cursor::<ListItemSummary>(());
        cursor.seek(&Count(scroll_top.item_ix), Bias::Right);
        let start_y = cursor.start().height + scroll_top.offset_in_item;
        cursor.seek_forward(&Height(start_y + height), Bias::Left);
        scroll_top.item_ix..cursor.start().count + 1
    }

    pub(super) fn scroll(
        &mut self,
        scroll_top: &ListOffset,
        height: Pixels,
        delta: Point<Pixels>,
        current_view: EntityId,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Drop scroll events after a reset, since we can't calculate
        // the new logical scroll top without the item heights
        if self.reset {
            return;
        }

        let padding = self.last_padding.unwrap_or_default();
        let scroll_max =
            (self.items.summary().height + padding.top + padding.bottom - height).max(px(0.));
        let new_scroll_top = (self.scroll_top(scroll_top) - delta.y)
            .max(px(0.))
            .min(scroll_max);

        if self.alignment == ListAlignment::Bottom && new_scroll_top == scroll_max {
            self.pending_scroll = None;
            self.logical_scroll_top = None;
        } else {
            let (start, ..) =
                self.items
                    .find::<ListItemSummary, _>((), &Height(new_scroll_top), Bias::Right);
            let scroll_top = ListOffset {
                item_ix: start.count,
                offset_in_item: new_scroll_top - start.height,
            };
            // The user's scroll supersedes the position stashed by a
            // remeasure; re-anchor the pending adjustment so it doesn't revert
            // this scroll on the next layout.
            self.rebase_pending_scroll(scroll_top);
            self.logical_scroll_top = Some(scroll_top);
        }

        if delta.y > px(0.) {
            self.follow_state.stop_following();
        }

        if let Some(handler) = self.scroll_handler.as_mut() {
            let visible_range = Self::visible_range(&self.items, height, scroll_top);
            handler(
                &ListScrollEvent {
                    visible_range,
                    count: self.items.summary().count,
                    is_scrolled: self.logical_scroll_top.is_some(),
                    is_following_tail: matches!(
                        self.follow_state,
                        FollowState::Tail { is_following: true }
                    ),
                },
                window,
                cx,
            );
        }

        cx.notify(current_view);
    }

    pub(super) fn logical_scroll_top(&self) -> ListOffset {
        self.logical_scroll_top
            .unwrap_or_else(|| match self.alignment {
                ListAlignment::Top => ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.),
                },
                ListAlignment::Bottom => ListOffset {
                    item_ix: self.items.summary().count,
                    offset_in_item: px(0.),
                },
            })
    }

    pub(super) fn scroll_top(&self, logical_scroll_top: &ListOffset) -> Pixels {
        let (start, ..) = self.items.find::<ListItemSummary, _>(
            (),
            &Count(logical_scroll_top.item_ix),
            Bias::Right,
        );
        start.height + logical_scroll_top.offset_in_item
    }

    // Scrollbar support

    pub(super) fn set_offset_from_scrollbar(&mut self, point: Point<Pixels>) {
        let Some(bounds) = self.last_layout_bounds else {
            return;
        };
        let height = bounds.size.height;

        let padding = self.last_padding.unwrap_or_default();
        // Scrollbar drag positions are computed from the content height
        // captured at drag start, so map them back using the same height.
        let content_height = self
            .scrollbar_drag_start_height
            .unwrap_or_else(|| self.items.summary().height);
        let scroll_max = (content_height + padding.top + padding.bottom - height).max(px(0.));
        let new_scroll_top = (-point.y).max(px(0.)).min(scroll_max);

        // If content grew during the drag, the frozen bottom is below the
        // live bottom. Treat dragging to the frozen end as resuming tail follow.
        let dragged_to_end =
            scroll_max > px(0.) && new_scroll_top >= (scroll_max - px(1.0)).max(px(0.));
        if dragged_to_end && matches!(self.follow_state, FollowState::Tail { .. }) {
            self.follow_state = FollowState::Tail { is_following: true };
            let item_count = self.items.summary().count;
            self.pending_scroll = None;
            self.logical_scroll_top = Some(ListOffset {
                item_ix: item_count,
                offset_in_item: px(0.),
            });
            return;
        }

        self.follow_state.stop_following();

        if self.alignment == ListAlignment::Bottom && new_scroll_top == scroll_max {
            self.pending_scroll = None;
            self.logical_scroll_top = None;
        } else {
            let (start, _, _) =
                self.items
                    .find::<ListItemSummary, _>((), &Height(new_scroll_top), Bias::Right);

            let scroll_top = ListOffset {
                item_ix: start.count,
                offset_in_item: new_scroll_top - start.height,
            };
            self.rebase_pending_scroll(scroll_top);
            self.logical_scroll_top = Some(scroll_top);
        }
    }
}

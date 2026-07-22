use super::*;

impl ListState {
    /// Construct a new list state, for storage on a view.
    ///
    /// The overdraw parameter controls how much extra space is rendered
    /// above and below the visible area. Elements within this area will
    /// be measured even though they are not visible. This can help ensure
    /// that the list doesn't flicker or pop in when scrolling.
    pub fn new(item_count: usize, alignment: ListAlignment, overdraw: Pixels) -> Self {
        let this = Self(Rc::new(RefCell::new(StateInner {
            last_layout_bounds: None,
            last_padding: None,
            items: SumTree::default(),
            logical_scroll_top: None,
            alignment,
            overdraw,
            scroll_handler: None,
            reset: false,
            scrollbar_drag_start_height: None,
            measuring_behavior: ListMeasuringBehavior::default(),
            pending_scroll: None,
            follow_state: FollowState::default(),
        })));
        this.splice(0..0, item_count);
        this
    }

    /// Set the list to measure all items in the list in the first layout phase.
    ///
    /// This is useful for ensuring that the scrollbar size is correct instead of based on only rendered elements.
    pub fn measure_all(self) -> Self {
        self.0.borrow_mut().measuring_behavior = ListMeasuringBehavior::Measure(false);
        self
    }

    /// Reset this instantiation of the list state.
    ///
    /// Note that this will cause scroll events to be dropped until the next paint.
    pub fn reset(&self, element_count: usize) {
        let old_count = {
            let state = &mut *self.0.borrow_mut();
            state.reset = true;
            state.measuring_behavior.reset();
            state.logical_scroll_top = None;
            state.pending_scroll = None;
            state.scrollbar_drag_start_height = None;
            state.items.summary().count
        };

        self.splice(0..old_count, element_count);
    }

    /// Remeasure all items while preserving proportional scroll position.
    ///
    /// Use this when item heights may have changed (e.g., font size changes)
    /// but the number and identity of items remains the same.
    pub fn remeasure(&self) {
        let count = self.item_count();
        self.remeasure_items_with_scroll_anchor(0..count, ScrollAnchor::Proportional);
    }

    /// Mark items in `range` as needing remeasurement while preserving
    /// the current scroll position. Unlike [`Self::splice`], this does
    /// not change the number of items or blow away `logical_scroll_top`.
    ///
    /// Use this when an item's content has changed and its rendered
    /// height may be different (e.g., streaming text, tool results
    /// loading), but the item itself still exists at the same index.
    pub fn remeasure_items(&self, range: Range<usize>) {
        self.remeasure_items_with_scroll_anchor(range, ScrollAnchor::Absolute);
    }

    fn remeasure_items_with_scroll_anchor(&self, range: Range<usize>, scroll_anchor: ScrollAnchor) {
        let state = &mut *self.0.borrow_mut();

        if let Some(scroll_top) = state.logical_scroll_top {
            if range.contains(&scroll_top.item_ix) {
                state.pending_scroll = match scroll_anchor {
                    ScrollAnchor::Absolute => Some(PendingScroll::Absolute {
                        item_ix: scroll_top.item_ix,
                        offset: scroll_top.offset_in_item,
                    }),
                    ScrollAnchor::Proportional => {
                        // If the scroll-top item falls within the remeasured range,
                        // store a fractional offset so the layout can restore the
                        // proportional scroll position after the item is re-rendered
                        // at its new height.
                        let mut cursor = state.items.cursor::<Count>(());
                        cursor.seek(&Count(scroll_top.item_ix), Bias::Right);

                        cursor
                            .item()
                            .and_then(|item| {
                                item.size().map(|size| {
                                    let fraction = if size.height.0 > 0.0 {
                                        (scroll_top.offset_in_item.0 / size.height.0)
                                            .clamp(0.0, 1.0)
                                    } else {
                                        0.0
                                    };

                                    PendingScroll::Proportional(PendingScrollFraction {
                                        item_ix: scroll_top.item_ix,
                                        fraction,
                                    })
                                })
                            })
                            .or_else(|| state.pending_scroll.clone())
                    }
                };
            }
        }

        // Rebuild the tree, replacing items in the range with
        // Unmeasured copies that keep their focus handles.
        let new_items = {
            let mut cursor = state.items.cursor::<Count>(());
            let mut new_items = cursor.slice(&Count(range.start), Bias::Right);
            let invalidated = cursor.slice(&Count(range.end), Bias::Right);
            new_items.extend(
                invalidated.iter().map(|item| ListItem::Unmeasured {
                    size_hint: item.size_hint(),
                    focus_handle: item.focus_handle(),
                }),
                (),
            );
            new_items.append(cursor.suffix(), ());
            new_items
        };
        state.items = new_items;
        state.measuring_behavior.reset();
    }

    /// The number of items in this list.
    pub fn item_count(&self) -> usize {
        self.0.borrow().items.summary().count
    }

    /// Whether the list is scrolled to the end, or `None` if the list is
    /// not scrollable or the total content height is not yet known.
    pub fn is_scrolled_to_end(&self) -> Option<bool> {
        let state = self.0.borrow();
        let bounds = state.last_layout_bounds?;
        let summary = state.items.summary();
        if summary.has_unknown_height {
            return None;
        }
        let padding = state.last_padding.unwrap_or_default();
        let content_height = summary.height + padding.top + padding.bottom;
        let scroll_max = (content_height - bounds.size.height).max(px(0.));
        if scroll_max <= px(0.) {
            return None;
        }
        let scroll_top = state.scroll_top(&state.logical_scroll_top());
        Some(scroll_top >= scroll_max)
    }

    /// Inform the list state that the items in `old_range` have been replaced
    /// by `count` new items that must be recalculated.
    pub fn splice(&self, old_range: Range<usize>, count: usize) {
        self.splice_focusable(old_range, (0..count).map(|_| None))
    }

    /// Register with the list state that the items in `old_range` have been replaced
    /// by new items. As opposed to [`Self::splice`], this method allows an iterator of optional focus handles
    /// to be supplied to properly integrate with items in the list that can be focused. If a focused item
    /// is scrolled out of view, the list will continue to render it to allow keyboard interaction.
    pub fn splice_focusable(
        &self,
        old_range: Range<usize>,
        focus_handles: impl IntoIterator<Item = Option<FocusHandle>>,
    ) {
        let state = &mut *self.0.borrow_mut();

        let mut old_items = state.items.cursor::<Count>(());
        let mut new_items = old_items.slice(&Count(old_range.start), Bias::Right);
        old_items.seek_forward(&Count(old_range.end), Bias::Right);

        let mut spliced_count = 0;
        new_items.extend(
            focus_handles.into_iter().map(|focus_handle| {
                spliced_count += 1;
                ListItem::Unmeasured {
                    size_hint: None,
                    focus_handle,
                }
            }),
            (),
        );
        new_items.append(old_items.suffix(), ());
        drop(old_items);
        state.items = new_items;

        if let Some(ListOffset {
            item_ix,
            offset_in_item,
        }) = state.logical_scroll_top.as_mut()
        {
            if old_range.contains(item_ix) {
                *item_ix = old_range.start;
                *offset_in_item = px(0.);
            } else if old_range.end <= *item_ix {
                *item_ix = *item_ix - (old_range.end - old_range.start) + spliced_count;
            }
        }
    }

    /// Set a handler that will be called when the list is scrolled.
    pub fn set_scroll_handler(
        &self,
        handler: impl FnMut(&ListScrollEvent, &mut Window, &mut App) + 'static,
    ) {
        self.0.borrow_mut().scroll_handler = Some(Box::new(handler))
    }

    /// Get the current scroll offset, in terms of the list's items.
    pub fn logical_scroll_top(&self) -> ListOffset {
        self.0.borrow().logical_scroll_top()
    }

    /// Scroll the list by the given offset
    pub fn scroll_by(&self, distance: Pixels) {
        if distance == px(0.) {
            return;
        }

        let current_offset = self.logical_scroll_top();
        let state = &mut *self.0.borrow_mut();

        if distance < px(0.) {
            state.follow_state.stop_following();
        }

        let mut cursor = state.items.cursor::<ListItemSummary>(());
        cursor.seek(&Count(current_offset.item_ix), Bias::Right);

        let start_pixel_offset = cursor.start().height + current_offset.offset_in_item;
        let new_pixel_offset = (start_pixel_offset + distance).max(px(0.));
        if new_pixel_offset > start_pixel_offset {
            cursor.seek_forward(&Height(new_pixel_offset), Bias::Right);
        } else {
            cursor.seek(&Height(new_pixel_offset), Bias::Right);
        }

        let scroll_top = ListOffset {
            item_ix: cursor.start().count,
            offset_in_item: new_pixel_offset - cursor.start().height,
        };
        drop(cursor);
        state.rebase_pending_scroll(scroll_top);
        state.logical_scroll_top = Some(scroll_top);
    }

    /// Scroll the list to the very end (past the last item).
    ///
    /// Unlike [`scroll_to_reveal_item`], this uses the total item count as the
    /// anchor, so the list's layout pass will walk backwards from the end and
    /// always show the bottom of the last item — even when that item is still
    /// growing (e.g. during streaming).
    pub fn scroll_to_end(&self) {
        let state = &mut *self.0.borrow_mut();
        let item_count = state.items.summary().count;
        state.pending_scroll = None;
        state.logical_scroll_top = Some(ListOffset {
            item_ix: item_count,
            offset_in_item: px(0.),
        });
    }

    /// Set the follow mode for the list. In `Tail` mode, the list
    /// will auto-scroll to the end and re-engage after the user
    /// scrolls back to the bottom. In `Normal` mode, no automatic
    /// following occurs.
    pub fn set_follow_mode(&self, mode: FollowMode) {
        let state = &mut *self.0.borrow_mut();

        match mode {
            FollowMode::Normal => {
                state.follow_state = FollowState::Normal;
            }
            FollowMode::Tail => {
                state.follow_state = FollowState::Tail { is_following: true };
                if matches!(mode, FollowMode::Tail) {
                    let item_count = state.items.summary().count;
                    state.logical_scroll_top = Some(ListOffset {
                        item_ix: item_count,
                        offset_in_item: px(0.),
                    });
                }
            }
        }
    }

    /// Returns whether the list is currently actively following the
    /// tail (snapping to the end on each layout).
    pub fn is_following_tail(&self) -> bool {
        matches!(
            self.0.borrow().follow_state,
            FollowState::Tail { is_following: true }
        )
    }

    /// Scroll the list to the given offset
    pub fn scroll_to(&self, mut scroll_top: ListOffset) {
        let state = &mut *self.0.borrow_mut();
        let item_count = state.items.summary().count;
        if scroll_top.item_ix >= item_count {
            scroll_top.item_ix = item_count;
            scroll_top.offset_in_item = px(0.);
        }

        if scroll_top.item_ix < item_count {
            state.follow_state.stop_following();
        }

        state.rebase_pending_scroll(scroll_top);
        state.logical_scroll_top = Some(scroll_top);
    }

    /// Scroll the list to the given item, such that the item is fully visible.
    pub fn scroll_to_reveal_item(&self, ix: usize) {
        let state = &mut *self.0.borrow_mut();

        let mut scroll_top = state.logical_scroll_top();
        let height = state
            .last_layout_bounds
            .map_or(px(0.), |bounds| bounds.size.height);
        let padding = state.last_padding.unwrap_or_default();

        if ix <= scroll_top.item_ix {
            scroll_top.item_ix = ix;
            scroll_top.offset_in_item = px(0.);
        } else {
            let mut cursor = state.items.cursor::<ListItemSummary>(());
            cursor.seek(&Count(ix + 1), Bias::Right);
            let bottom = cursor.start().height + padding.top;
            let goal_top = px(0.).max(bottom - height + padding.bottom);

            cursor.seek(&Height(goal_top), Bias::Left);
            let start_ix = cursor.start().count;
            let start_item_top = cursor.start().height;

            if start_ix >= scroll_top.item_ix {
                scroll_top.item_ix = start_ix;
                scroll_top.offset_in_item = goal_top - start_item_top;
            }
        }

        state.rebase_pending_scroll(scroll_top);
        state.logical_scroll_top = Some(scroll_top);
    }

    /// Get the bounds for the given item in window coordinates, if it's
    /// been rendered.
    pub fn bounds_for_item(&self, ix: usize) -> Option<Bounds<Pixels>> {
        let state = &*self.0.borrow();

        let bounds = state.last_layout_bounds.unwrap_or_default();
        let scroll_top = state.logical_scroll_top();
        if ix < scroll_top.item_ix {
            return None;
        }

        let mut cursor = state.items.cursor::<Dimensions<Count, Height>>(());
        cursor.seek(&Count(scroll_top.item_ix), Bias::Right);

        let scroll_top = cursor.start().1.0 + scroll_top.offset_in_item;

        cursor.seek_forward(&Count(ix), Bias::Right);
        if let Some(&ListItem::Measured { size, .. }) = cursor.item() {
            let &Dimensions(Count(count), Height(top), _) = cursor.start();
            if count == ix {
                let top = bounds.top() + top - scroll_top;
                return Some(Bounds::from_corners(
                    point(bounds.left(), top),
                    point(bounds.right(), top + size.height),
                ));
            }
        }
        None
    }

    /// Call this method when the user starts dragging the scrollbar.
    ///
    /// This will prevent the height reported to the scrollbar from changing during the drag
    /// as items in the overdraw get measured, and help offset scroll position changes accordingly.
    pub fn scrollbar_drag_started(&self) {
        let mut state = self.0.borrow_mut();
        state.scrollbar_drag_start_height = Some(state.items.summary().height);
    }

    /// Called when the user stops dragging the scrollbar.
    ///
    /// See `scrollbar_drag_started`.
    pub fn scrollbar_drag_ended(&self) {
        self.0.borrow_mut().scrollbar_drag_start_height.take();
    }

    /// Returns `true` if the scrollbar is currently being dragged.
    ///
    /// This is set between [`scrollbar_drag_started`](Self::scrollbar_drag_started)
    /// and [`scrollbar_drag_ended`](Self::scrollbar_drag_ended) calls. Useful for
    /// consumers that need to distinguish scrollbar drags from wheel/trackpad scrolls,
    /// e.g. to suppress auto-scroll behavior during manual positioning.
    pub fn is_scrollbar_dragging(&self) -> bool {
        self.0.borrow().scrollbar_drag_start_height.is_some()
    }

    /// Set the offset from the scrollbar
    pub fn set_offset_from_scrollbar(&self, point: Point<Pixels>) {
        self.0.borrow_mut().set_offset_from_scrollbar(point);
    }

    /// Returns the maximum scroll offset according to the items we have measured.
    /// This value remains constant while dragging to prevent the scrollbar from moving away unexpectedly.
    pub fn max_offset_for_scrollbar(&self) -> Point<Pixels> {
        let state = self.0.borrow();
        point(Pixels::ZERO, state.max_scroll_offset())
    }

    /// Returns the current scroll offset adjusted for the scrollbar.
    ///
    /// The returned offset has a negative `y` component representing
    /// how far the content has scrolled.
    pub fn scroll_px_offset_for_scrollbar(&self) -> Point<Pixels> {
        let state = &self.0.borrow();

        if state.logical_scroll_top.is_none() && state.alignment == ListAlignment::Bottom {
            return Point::new(px(0.), -state.max_scroll_offset());
        }

        let logical_scroll_top = state.logical_scroll_top();

        let mut cursor = state.items.cursor::<ListItemSummary>(());
        let summary: ListItemSummary =
            cursor.summary(&Count(logical_scroll_top.item_ix), Bias::Right);
        let offset = summary.height + logical_scroll_top.offset_in_item;

        Point::new(px(0.), -offset)
    }

    /// Return the bounds of the viewport in pixels.
    pub fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().last_layout_bounds.unwrap_or_default()
    }

    /// Returns whether the item is entirely above the viewport, or `None` if
    /// the list has not measured enough layout to know.
    ///
    /// A zero-height viewport still yields a definitive answer: callers may
    /// size sibling UI based on this query (potentially squeezing the list
    /// itself to zero height), so returning `None` in that case would make
    /// the answer oscillate from frame to frame.
    pub fn item_is_above_viewport(&self, ix: usize) -> Option<bool> {
        let viewport_bounds = self.0.borrow().last_layout_bounds?;

        let scroll_top = self.logical_scroll_top();
        if ix < scroll_top.item_ix {
            // Rows before the logical scroll top have no item bounds, but
            // their position relative to the viewport is known from scroll state.
            return Some(true);
        }

        let item_bounds = self.bounds_for_item(ix)?;
        Some(item_bounds.bottom() <= viewport_bounds.top())
    }

    /// Returns whether the item is entirely below the viewport, or `None` if
    /// the list has not measured enough layout to know.
    ///
    /// See [`Self::item_is_above_viewport`] for why a zero-height viewport
    /// still yields a definitive answer.
    pub fn item_is_below_viewport(&self, ix: usize) -> Option<bool> {
        let viewport_bounds = self.0.borrow().last_layout_bounds?;

        let scroll_top = self.logical_scroll_top();
        if ix < scroll_top.item_ix {
            // Rows before the logical scroll top have no item bounds, but
            // their position relative to the viewport is known from scroll state.
            return Some(false);
        }

        let item_bounds = self.bounds_for_item(ix)?;
        Some(item_bounds.top() >= viewport_bounds.bottom())
    }
}

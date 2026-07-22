use super::*;

fn test_follow_tail_stays_at_bottom_as_items_grow(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    // 10 items, each 50px tall → 500px total content, 200px viewport.
    // With follow-tail on, the list should always show the bottom.
    let item_height = Rc::new(Cell::new(50usize));
    let state = ListState::new(10, crate::ListAlignment::Top, px(0.));

    struct TestView {
        state: ListState,
        item_height: Rc<Cell<usize>>,
    }
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let height = self.item_height.get();
            list(self.state.clone(), move |_, _, _| {
                div().h(px(height as f32)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    let state_clone = state.clone();
    let item_height_clone = item_height.clone();
    let view = cx.update(|_, cx| {
        cx.new(|_| TestView {
            state: state_clone,
            item_height: item_height_clone,
        })
    });

    state.set_follow_mode(FollowMode::Tail);

    // First paint — items are 50px, total 500px, viewport 200px.
    // Follow-tail should anchor to the end.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    // The scroll should be at the bottom: the last visible items fill the
    // 200px viewport from the end of 500px of content (offset 300px).
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 6);
    assert_eq!(offset.offset_in_item, px(0.));
    assert!(state.is_following_tail());

    // Simulate items growing (e.g. streaming content makes each item taller).
    // 10 items × 80px = 800px total.
    item_height.set(80);
    state.remeasure();

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    // After growth, follow-tail should have re-anchored to the new end.
    // 800px total − 200px viewport = 600px offset → item 7 at offset 40px,
    // but follow-tail anchors to item_count (10), and layout walks back to
    // fill 200px, landing at item 7 (7 × 80 = 560, 800 − 560 = 240 > 200,
    // so item 8: 8 × 80 = 640, 800 − 640 = 160 < 200 → keeps walking →
    // item 7: offset = 800 − 200 = 600, item_ix = 600/80 = 7, remainder 40).
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 7);
    assert_eq!(offset.offset_in_item, px(40.));
    assert!(state.is_following_tail());
}

#[gpui::test]
fn test_follow_tail_disengages_on_user_scroll(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    // 10 items × 50px = 500px total, 200px viewport.
    let state = ListState::new(10, crate::ListAlignment::Top, px(0.));

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(50.)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    state.set_follow_mode(FollowMode::Tail);

    // Paint with follow-tail — scroll anchored to the bottom.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, cx| {
        cx.new(|_| TestView(state.clone())).into_any_element()
    });
    assert!(state.is_following_tail());

    // Simulate the user scrolling up.
    // This should disengage follow-tail.
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(100.))),
        ..Default::default()
    });

    assert!(
        !state.is_following_tail(),
        "follow-tail should disengage when the user scrolls toward the start"
    );
}

#[gpui::test]
fn test_follow_tail_disengages_on_scrollbar_reposition(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    // 10 items × 50px = 500px total, 200px viewport.
    let state = ListState::new(10, crate::ListAlignment::Top, px(0.)).measure_all();

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(50.)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    let view = cx.update(|_, cx| cx.new(|_| TestView(state.clone())));

    state.set_follow_mode(FollowMode::Tail);

    // Paint with follow-tail — scroll anchored to the bottom.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert!(state.is_following_tail());

    // Simulate the scrollbar moving the viewport to the middle.
    state.set_offset_from_scrollbar(point(px(0.), px(-150.)));

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 3);
    assert_eq!(offset.offset_in_item, px(0.));
    assert!(
        !state.is_following_tail(),
        "follow-tail should disengage when the scrollbar manually repositions the list"
    );

    // A subsequent draw should preserve the user's manual position instead
    // of snapping back to the end.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 3);
    assert_eq!(offset.offset_in_item, px(0.));
}

#[gpui::test]
fn test_scrollbar_drag_with_growing_content(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let last_item_height = Rc::new(Cell::new(50usize));
    let state = ListState::new(10, crate::ListAlignment::Top, px(0.)).measure_all();

    struct TestView {
        state: ListState,
        last_item_height: Rc<Cell<usize>>,
    }
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let last_item_height = self.last_item_height.clone();
            list(self.state.clone(), move |index, _, _| {
                let height = if index == 9 {
                    last_item_height.get()
                } else {
                    50
                };
                div().h(px(height as f32)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    let view = cx.update(|_, cx| {
        cx.new(|_| TestView {
            state: state.clone(),
            last_item_height: last_item_height.clone(),
        })
    });

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    state.scrollbar_drag_started();

    state.set_offset_from_scrollbar(point(px(0.), px(-150.)));
    let scrollbar_offset_before_growth = state.scroll_px_offset_for_scrollbar();

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 3);
    assert_eq!(offset.offset_in_item, px(0.));

    last_item_height.set(550);
    state.remeasure_items(9..10);
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    assert_eq!(state.max_offset_for_scrollbar().y, px(300.));
    assert_eq!(
        state.scroll_px_offset_for_scrollbar(),
        scrollbar_offset_before_growth
    );

    state.set_offset_from_scrollbar(point(px(0.), px(-150.)));
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 3);
    assert_eq!(offset.offset_in_item, px(0.));
}

#[gpui::test]
fn test_set_follow_tail_snaps_to_bottom(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    // 10 items × 50px = 500px total, 200px viewport.
    let state = ListState::new(10, crate::ListAlignment::Top, px(0.));

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(50.)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    let view = cx.update(|_, cx| cx.new(|_| TestView(state.clone())));

    // Scroll to the middle of the list (item 3).
    state.scroll_to(gpui::ListOffset {
        item_ix: 3,
        offset_in_item: px(0.),
    });

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 3);
    assert_eq!(offset.offset_in_item, px(0.));
    assert!(!state.is_following_tail());

    // Enable follow-tail — this should immediately snap the scroll anchor
    // to the end, like the user just sent a prompt.
    state.set_follow_mode(FollowMode::Tail);

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    // After paint, scroll should be at the bottom.
    // 500px total − 200px viewport = 300px offset → item 6, offset 0.
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 6);
    assert_eq!(offset.offset_in_item, px(0.));
    assert!(state.is_following_tail());
}

#[gpui::test]
fn test_bottom_aligned_scrollbar_offset_at_end(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    const ITEMS: usize = 10;
    const ITEM_SIZE: f32 = 50.0;

    let state = ListState::new(
        ITEMS,
        crate::ListAlignment::Bottom,
        px(ITEMS as f32 * ITEM_SIZE),
    );

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(ITEM_SIZE)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, cx| {
        cx.new(|_| TestView(state.clone())).into_any_element()
    });

    // Bottom-aligned lists start pinned to the end: logical_scroll_top returns
    // item_ix == item_count, meaning no explicit scroll position has been set.
    assert_eq!(state.logical_scroll_top().item_ix, ITEMS);

    let max_offset = state.max_offset_for_scrollbar();
    let scroll_offset = state.scroll_px_offset_for_scrollbar();

    assert_eq!(
        -scroll_offset.y, max_offset.y,
        "scrollbar offset ({}) should equal max offset ({}) when list is pinned to bottom",
        -scroll_offset.y, max_offset.y,
    );
}

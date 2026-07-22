use super::*;

#[gpui::test]
fn test_measure_all_after_width_change(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

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

    // First draw at width 100: all 10 items measured (total 500px).
    // Viewport is 200px, so max scroll offset should be 300px.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert_eq!(state.max_offset_for_scrollbar().y, px(300.));

    // Second draw at a different width: items get invalidated.
    // Without the fix, max_offset would drop because unmeasured items
    // contribute 0 height.
    cx.draw(point(px(0.), px(0.)), size(px(200.), px(200.)), |_, _| {
        view.into_any_element()
    });
    assert_eq!(state.max_offset_for_scrollbar().y, px(300.));
}

#[gpui::test]
fn test_remeasure(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    // Create a list with 10 items, each 100px tall. We'll keep a reference
    // to the item height so we can later change the height and assert how
    // `ListState` handles it.
    let item_height = Rc::new(Cell::new(100usize));
    let state = ListState::new(10, crate::ListAlignment::Top, px(10.));

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

    // Simulate scrolling 40px inside the element with index 2. Since the
    // original item height is 100px, this equates to 40% inside the item.
    state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(40.),
    });

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 2);
    assert_eq!(offset.offset_in_item, px(40.));

    // Update the `item_height` to be 50px instead of 100px so we can assert
    // that the scroll position is proportionally preserved, that is,
    // instead of 40px from the top of item 2, it should be 20px, since the
    // item's height has been halved.
    item_height.set(50);
    state.remeasure();

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 2);
    assert_eq!(offset.offset_in_item, px(20.));
}

#[gpui::test]
fn test_remeasure_item_preserves_scroll_offset(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let item_height = Rc::new(Cell::new(100usize));
    let state = ListState::new(20, crate::ListAlignment::Top, px(10.));

    struct TestView {
        state: ListState,
        item_height: Rc<Cell<usize>>,
    }

    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let height = self.item_height.get();
            list(self.state.clone(), move |index, _, _| {
                let height = if index == 5 { height } else { 100 };
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

    state.scroll_to(gpui::ListOffset {
        item_ix: 5,
        offset_in_item: px(40.),
    });

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    item_height.set(200);
    state.remeasure_items(5..6);

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 5);
    assert_eq!(offset.offset_in_item, px(40.));
}

#[gpui::test]
fn test_remeasure_then_scroll_does_not_revert_scroll_position(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(20, crate::ListAlignment::Top, px(10.));

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(100.)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    let view = {
        let state = state.clone();
        cx.update(|_, cx| cx.new(|_| TestView(state)))
    };

    state.scroll_to(gpui::ListOffset {
        item_ix: 5,
        offset_in_item: px(40.),
    });

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    state.remeasure_items(5..6);

    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-30.))),
        ..Default::default()
    });

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 5);
    assert_eq!(offset.offset_in_item, px(70.));

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 5);
    assert_eq!(
        offset.offset_in_item,
        px(70.),
        "scrolling after a remeasure should not be reverted by the stale pending scroll"
    );
}

#[gpui::test]
fn test_scroll_after_remeasure_clamps_to_shrunk_item_height(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let item_height = Rc::new(Cell::new(100usize));
    let state = ListState::new(20, crate::ListAlignment::Top, px(10.));

    struct TestView {
        state: ListState,
        item_height: Rc<Cell<usize>>,
    }

    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let height = self.item_height.get();
            list(self.state.clone(), move |index, _, _| {
                let height = if index == 5 { height } else { 100 };
                div().h(px(height as f32)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    let view = {
        let state = state.clone();
        let item_height = item_height.clone();
        cx.update(|_, cx| cx.new(|_| TestView { state, item_height }))
    };

    state.scroll_to(gpui::ListOffset {
        item_ix: 5,
        offset_in_item: px(40.),
    });

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    // Item 5 shrinks from 100px to 50px and is remeasured...
    item_height.set(50);
    state.remeasure_items(5..6);

    // ...and then the user scrolls down by 30px before the next frame,
    // landing at offset 70.
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-30.))),
        ..Default::default()
    });

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    // The rebased pending scroll clamps the user's offset to the item's
    // new height instead of leaving it pointing past the end of the item.
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 5);
    assert_eq!(offset.offset_in_item, px(50.));
}

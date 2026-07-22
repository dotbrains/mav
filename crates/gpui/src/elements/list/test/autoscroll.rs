use super::*;

#[gpui::test]
fn test_autoscroll_above_item_top_renders_items_above(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.));
    state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(0.),
    });

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |ix, _, _| {
                if ix == 2 {
                    // Request an autoscroll whose top sits 30px above item 2's
                    // own top, mimicking a scroll-margin overshoot.
                    canvas(
                        |bounds, window, _| {
                            window.request_autoscroll(Bounds::from_corners(
                                point(bounds.left(), bounds.top() - px(30.)),
                                point(bounds.right(), bounds.top() + px(5.)),
                            ));
                        },
                        |_, _, _, _| {},
                    )
                    .h(px(20.))
                    .w_full()
                    .into_any()
                } else {
                    div().h(px(20.)).w_full().into_any()
                }
            })
            .w_full()
            .h_full()
        }
    }

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(60.)), |_, cx| {
        cx.new(|_| TestView(state.clone())).into_any_element()
    });

    // 30px above item 2's top, with 20px items, lands 10px into item 0.
    let scroll_top = state.logical_scroll_top();
    assert!(
        scroll_top.offset_in_item >= px(0.),
        "offset_in_item must never be negative (would leave blank space above), got {:?}",
        scroll_top.offset_in_item,
    );
    assert_eq!(scroll_top.item_ix, 0);
    assert_eq!(scroll_top.offset_in_item, px(10.));
}

#[gpui::test]
fn test_reset_after_paint_before_scroll(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.));

    // Ensure that the list is scrolled to the top
    state.scroll_to(gpui::ListOffset {
        item_ix: 0,
        offset_in_item: px(0.0),
    });

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(10.)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    // Paint
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(20.)), |_, cx| {
        cx.new(|_| TestView(state.clone())).into_any_element()
    });

    // Reset
    state.reset(5);

    // And then receive a scroll event _before_ the next paint
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(1.), px(1.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-500.))),
        ..Default::default()
    });

    // Scroll position should stay at the top of the list
    assert_eq!(state.logical_scroll_top().item_ix, 0);
    assert_eq!(state.logical_scroll_top().offset_in_item, px(0.));
}

#[gpui::test]
fn test_scroll_by_positive_and_negative_distance(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.));

    struct TestView(ListState);
    impl Render for TestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            list(self.0.clone(), |_, _, _| {
                div().h(px(20.)).w_full().into_any()
            })
            .w_full()
            .h_full()
        }
    }

    // Paint
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(100.)), |_, cx| {
        cx.new(|_| TestView(state.clone())).into_any_element()
    });

    // Test positive distance: start at item 1, move down 30px
    state.scroll_by(px(30.));

    // Should move to item 2
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 1);
    assert_eq!(offset.offset_in_item, px(10.));

    // Test negative distance: start at item 2, move up 30px
    state.scroll_by(px(-30.));

    // Should move back to item 1
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 0);
    assert_eq!(offset.offset_in_item, px(0.));

    // Test zero distance
    state.scroll_by(px(0.));
    let offset = state.logical_scroll_top();
    assert_eq!(offset.item_ix, 0);
    assert_eq!(offset.offset_in_item, px(0.));
}

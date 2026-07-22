use super::*;

struct TestListView(ListState);
impl Render for TestListView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        list(self.0.clone(), |_, _, _| {
            div().h(px(20.)).w_full().into_any()
        })
        .w_full()
        .h_full()
    }
}

#[gpui::test]
fn test_item_viewport_queries_return_none_before_layout(_cx: &mut TestAppContext) {
    let state = ListState::new(5, crate::ListAlignment::Top, px(10.)).measure_all();

    assert_eq!(state.item_is_above_viewport(0), None);
    assert_eq!(state.item_is_below_viewport(0), None);
}

#[gpui::test]
fn test_item_viewport_queries_before_logical_scroll_top(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.)).measure_all();

    state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(0.),
    });
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(20.)), |_, cx| {
        cx.new(|_| TestListView(state.clone())).into_any_element()
    });

    assert_eq!(state.item_is_above_viewport(1), Some(true));
    assert_eq!(state.item_is_below_viewport(1), Some(false));
}

#[gpui::test]
fn test_item_viewport_queries_measured_item_inside_viewport(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.)).measure_all();

    state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(0.),
    });
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(20.)), |_, cx| {
        cx.new(|_| TestListView(state.clone())).into_any_element()
    });

    assert_eq!(state.item_is_above_viewport(2), Some(false));
    assert_eq!(state.item_is_below_viewport(2), Some(false));
}

#[gpui::test]
fn test_item_viewport_queries_measured_item_above_viewport(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.)).measure_all();

    state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(20.),
    });
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(20.)), |_, cx| {
        cx.new(|_| TestListView(state.clone())).into_any_element()
    });

    assert_eq!(state.item_is_above_viewport(2), Some(true));
    assert_eq!(state.item_is_below_viewport(2), Some(false));
}

#[gpui::test]
fn test_item_viewport_queries_measured_item_below_viewport(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.)).measure_all();

    state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(0.),
    });
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(20.)), |_, cx| {
        cx.new(|_| TestListView(state.clone())).into_any_element()
    });

    assert_eq!(state.item_is_above_viewport(3), Some(false));
    assert_eq!(state.item_is_below_viewport(3), Some(true));
}

#[gpui::test]
fn test_item_viewport_queries_remain_stable_with_zero_height_viewport(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.)).measure_all();

    state.scroll_to(gpui::ListOffset {
        item_ix: 2,
        offset_in_item: px(0.),
    });
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(20.)), |_, cx| {
        cx.new(|_| TestListView(state.clone())).into_any_element()
    });

    assert_eq!(state.item_is_above_viewport(3), Some(false));
    assert_eq!(state.item_is_below_viewport(3), Some(true));

    // Squeeze the list to zero height, e.g. because a sibling element
    // (sized based on the queries above) consumed all the space. The
    // answers must remain definitive rather than becoming `None`,
    // otherwise the sibling's size can oscillate between frames.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(0.)), |_, cx| {
        cx.new(|_| TestListView(state.clone())).into_any_element()
    });

    assert_eq!(state.item_is_above_viewport(1), Some(true));
    assert_eq!(state.item_is_below_viewport(1), Some(false));
    assert_eq!(state.item_is_above_viewport(3), Some(false));
    assert_eq!(state.item_is_below_viewport(3), Some(true));
}

#[gpui::test]
fn test_item_viewport_queries_after_scroll_to_end_before_layout(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    let state = ListState::new(5, crate::ListAlignment::Top, px(10.)).measure_all();

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(20.)), |_, cx| {
        cx.new(|_| TestListView(state.clone())).into_any_element()
    });

    state.scroll_to_end();

    assert_eq!(state.logical_scroll_top().item_ix, state.item_count());
    assert_eq!(state.item_is_above_viewport(0), Some(true));
    assert_eq!(state.item_is_below_viewport(0), Some(false));
}

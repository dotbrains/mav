use super::*;

/// When the user scrolls away from the bottom during follow_tail,
/// follow_tail suspends. If they scroll back to the bottom, the
/// next paint should re-engage follow_tail using fresh measurements.
#[gpui::test]
fn test_follow_tail_reengages_when_scrolled_back_to_bottom(cx: &mut TestAppContext) {
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

    state.set_follow_mode(FollowMode::Tail);

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert!(state.is_following_tail());

    // Scroll up — follow_tail should suspend (not fully disengage).
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(50.))),
        ..Default::default()
    });
    assert!(!state.is_following_tail());

    // Scroll back down to the bottom.
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-10000.))),
        ..Default::default()
    });

    // After a paint, follow_tail should re-engage because the
    // layout confirmed we're at the true bottom.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert!(
        state.is_following_tail(),
        "follow_tail should re-engage after scrolling back to the bottom"
    );
}

/// When an item is spliced to unmeasured (0px) while follow_tail
/// is suspended, the re-engagement check should still work correctly
#[gpui::test]
fn test_follow_tail_reengagement_not_fooled_by_unmeasured_items(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    // 20 items × 50px = 1000px total, 200px viewport, 1000px
    // overdraw so all items get measured during the follow_tail
    // paint (matching realistic production settings).
    let state = ListState::new(20, crate::ListAlignment::Top, px(1000.));

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

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert!(state.is_following_tail());

    // Scroll up a meaningful amount — suspends follow_tail.
    // 20 items × 50px = 1000px. viewport 200px. scroll_max = 800px.
    // Scrolling up 200px puts us at 600px, clearly not at bottom.
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(50.), px(100.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(200.))),
        ..Default::default()
    });
    assert!(!state.is_following_tail());

    // Invalidate the last item (simulates EntryUpdated calling
    // remeasure_items). This makes items.summary().height
    // temporarily wrong (0px for the invalidated item).
    state.remeasure_items(19..20);

    // Paint — layout re-measures the invalidated item with its true
    // height. The re-engagement check uses these fresh measurements.
    // Since we scrolled 200px up from the 800px max, we're at
    // ~600px — NOT at the bottom, so follow_tail should NOT
    // re-engage.
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert!(
        !state.is_following_tail(),
        "follow_tail should not falsely re-engage due to an unmeasured item \
             reducing items.summary().height"
    );
}

#[gpui::test]
fn test_follow_tail_reengages_after_scrollbar_disengagement(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();

    // 10 items × 50px = 500px total, 200px viewport, scroll_max = 300px.
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
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert!(state.is_following_tail());

    // Drag the scrollbar up to the middle — follow_tail should suspend.
    state.set_offset_from_scrollbar(point(px(0.), px(-150.)));
    assert!(!state.is_following_tail());

    // Drag the scrollbar back to the bottom — follow_tail should re-engage
    // on the next paint.
    state.set_offset_from_scrollbar(point(px(0.), px(-300.)));
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });
    assert!(
        state.is_following_tail(),
        "follow_tail should re-engage after scrolling back to the bottom via the scrollbar"
    );
}

#[gpui::test]
fn test_follow_tail_reengages_after_scrollbar_drag_to_bottom_while_growing(
    cx: &mut TestAppContext,
) {
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

    state.set_follow_mode(FollowMode::Tail);
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });
    assert!(state.is_following_tail());

    state.scrollbar_drag_started();

    state.splice(10..10, 10);
    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.clone().into_any_element()
    });

    state.set_offset_from_scrollbar(point(px(0.), px(-300.)));
    state.scrollbar_drag_ended();

    cx.draw(point(px(0.), px(0.)), size(px(100.), px(200.)), |_, _| {
        view.into_any_element()
    });

    assert!(
        state.is_following_tail(),
        "follow_tail should re-engage when the user drags the scrollbar to \
             the bottom of its track, even when content has grown during the drag \
             (so frozen_bottom < live_bottom)"
    );
}

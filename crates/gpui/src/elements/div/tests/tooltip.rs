use super::super::*;
use crate::elements::div::scroll::{ScrollActiveItem, ScrollStrategy};
use crate::{
    AppContext as _, Context, InputEvent, MouseMoveEvent, TestAppContext, util::FluentBuilder as _,
};
use std::rc::Weak;

struct TestTooltipView;

impl Render for TestTooltipView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(20.)).h(px(20.)).child("tooltip")
    }
}

type CapturedActiveTooltip = Rc<RefCell<Option<Weak<RefCell<Option<ActiveTooltip>>>>>>;

struct TooltipCaptureElement {
    child: AnyElement,
    captured_active_tooltip: CapturedActiveTooltip,
}

impl IntoElement for TooltipCaptureElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TooltipCaptureElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
        window.with_global_id("target".into(), |global_id, window| {
            window.with_element_state::<InteractiveElementState, _>(global_id, |state, _window| {
                let state = state.unwrap();
                *self.captured_active_tooltip.borrow_mut() =
                    state.active_tooltip.as_ref().map(Rc::downgrade);
                ((), state)
            })
        });
    }
}

struct TooltipOwner {
    captured_active_tooltip: CapturedActiveTooltip,
    show_delay_override: Option<Duration>,
}

impl Render for TooltipOwner {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        TooltipCaptureElement {
            child: div()
                .size_full()
                .child(
                    div()
                        .id("target")
                        .w(px(50.))
                        .h(px(50.))
                        .tooltip(|_, cx| cx.new(|_| TestTooltipView).into())
                        .when_some(self.show_delay_override, |this, delay| {
                            this.tooltip_show_delay(delay)
                        }),
                )
                .into_any_element(),
            captured_active_tooltip: self.captured_active_tooltip.clone(),
        }
    }
}

#[test]
fn scroll_handle_aligns_wide_children_to_left_edge() {
    let handle = ScrollHandle::new();
    {
        let mut state = handle.0.borrow_mut();
        state.bounds = Bounds::new(point(px(0.), px(0.)), size(px(80.), px(20.)));
        state.child_bounds = vec![Bounds::new(point(px(25.), px(0.)), size(px(200.), px(20.)))];
        state.overflow.x = Overflow::Scroll;
        state.active_item = Some(ScrollActiveItem {
            index: 0,
            strategy: ScrollStrategy::default(),
        });
    }

    handle.scroll_to_active_item();

    assert_eq!(handle.offset().x, px(-25.));
}

#[test]
fn scroll_handle_aligns_tall_children_to_top_edge() {
    let handle = ScrollHandle::new();
    {
        let mut state = handle.0.borrow_mut();
        state.bounds = Bounds::new(point(px(0.), px(0.)), size(px(20.), px(80.)));
        state.child_bounds = vec![Bounds::new(point(px(0.), px(25.)), size(px(20.), px(200.)))];
        state.overflow.y = Overflow::Scroll;
        state.active_item = Some(ScrollActiveItem {
            index: 0,
            strategy: ScrollStrategy::default(),
        });
    }

    handle.scroll_to_active_item();

    assert_eq!(handle.offset().y, px(-25.));
}

fn setup_tooltip_owner_test(
    show_delay_override: Option<Duration>,
) -> (
    TestAppContext,
    crate::AnyWindowHandle,
    CapturedActiveTooltip,
) {
    let mut test_app = TestAppContext::single();
    let captured_active_tooltip: CapturedActiveTooltip = Rc::new(RefCell::new(None));
    let window = test_app.add_window({
        let captured_active_tooltip = captured_active_tooltip.clone();
        move |_, _| TooltipOwner {
            captured_active_tooltip,
            show_delay_override,
        }
    });
    let any_window = window.into();

    test_app
        .update_window(any_window, |_, window, cx| {
            window.draw(cx).clear();
        })
        .unwrap();

    test_app
        .update_window(any_window, |_, window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position: point(px(10.), px(10.)),
                    modifiers: Default::default(),
                    pressed_button: None,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();

    test_app
        .update_window(any_window, |_, window, cx| {
            window.draw(cx).clear();
        })
        .unwrap();

    (test_app, any_window, captured_active_tooltip)
}

#[test]
fn tooltip_waiting_for_show_is_released_when_its_owner_disappears() {
    let (mut test_app, any_window, captured_active_tooltip) = setup_tooltip_owner_test(None);

    let weak_active_tooltip = captured_active_tooltip.borrow().clone().unwrap();
    let active_tooltip = weak_active_tooltip.upgrade().unwrap();
    assert!(matches!(
        active_tooltip.borrow().as_ref(),
        Some(ActiveTooltip::WaitingForShow { .. })
    ));

    test_app
        .update_window(any_window, |_, window, _| {
            window.remove_window();
        })
        .unwrap();
    test_app.run_until_parked();
    drop(active_tooltip);

    assert!(weak_active_tooltip.upgrade().is_none());
}

#[test]
fn tooltip_respects_custom_show_delay() {
    let extra_delay = Duration::from_secs(1);
    let show_delay_override = DEFAULT_TOOLTIP_SHOW_DELAY + extra_delay;
    let (mut test_app, _any_window, captured_active_tooltip) =
        setup_tooltip_owner_test(Some(show_delay_override));

    let weak_active_tooltip = captured_active_tooltip.borrow().clone().unwrap();
    let active_tooltip = weak_active_tooltip.upgrade().unwrap();

    test_app
        .dispatcher
        .advance_clock(DEFAULT_TOOLTIP_SHOW_DELAY);
    test_app.run_until_parked();

    assert!(matches!(
        active_tooltip.borrow().as_ref(),
        Some(ActiveTooltip::WaitingForShow { .. })
    ));

    test_app.dispatcher.advance_clock(extra_delay);
    test_app.run_until_parked();

    assert!(matches!(
        active_tooltip.borrow().as_ref(),
        Some(ActiveTooltip::Visible { .. })
    ));
}

#[test]
fn tooltip_is_released_when_its_owner_disappears() {
    let (mut test_app, any_window, captured_active_tooltip) = setup_tooltip_owner_test(None);

    let weak_active_tooltip = captured_active_tooltip.borrow().clone().unwrap();
    let active_tooltip = weak_active_tooltip.upgrade().unwrap();

    test_app
        .dispatcher
        .advance_clock(DEFAULT_TOOLTIP_SHOW_DELAY);
    test_app.run_until_parked();

    assert!(matches!(
        active_tooltip.borrow().as_ref(),
        Some(ActiveTooltip::Visible { .. })
    ));

    test_app
        .update_window(any_window, |_, window, _| {
            window.remove_window();
        })
        .unwrap();
    test_app.run_until_parked();
    drop(active_tooltip);

    assert!(weak_active_tooltip.upgrade().is_none());
}

#[test]
fn tooltip_hides_after_mouse_leaves_origin() {
    let (mut test_app, any_window, captured_active_tooltip) = setup_tooltip_owner_test(None);

    let weak_active_tooltip = captured_active_tooltip.borrow().clone().unwrap();
    let active_tooltip = weak_active_tooltip.upgrade().unwrap();

    test_app
        .dispatcher
        .advance_clock(DEFAULT_TOOLTIP_SHOW_DELAY);
    test_app.run_until_parked();

    assert!(matches!(
        active_tooltip.borrow().as_ref(),
        Some(ActiveTooltip::Visible { .. })
    ));

    test_app
        .update_window(any_window, |_, window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position: point(px(75.), px(75.)),
                    modifiers: Default::default(),
                    pressed_button: None,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();

    assert!(active_tooltip.borrow().is_none());
}

#[test]
fn test_write_a11y_info_string_and_numeric_properties() {
    let mut interactivity = Interactivity::default();
    interactivity.aria_label = Some("Buffer Font Size".into());
    interactivity.aria_value = Some("15".into());
    interactivity.aria_placeholder = Some("Search".into());
    interactivity.aria_numeric_value = Some(15.0);
    interactivity.aria_min_numeric_value = Some(6.0);
    interactivity.aria_max_numeric_value = Some(72.0);
    interactivity.aria_numeric_value_step = Some(1.0);

    let mut node = accesskit::Node::new(accesskit::Role::SpinButton);
    interactivity.write_a11y_info(&mut node);

    assert_eq!(node.label(), Some("Buffer Font Size"));
    assert_eq!(node.value(), Some("15"));
    assert_eq!(node.placeholder(), Some("Search"));
    assert_eq!(node.numeric_value(), Some(15.0));
    assert_eq!(node.min_numeric_value(), Some(6.0));
    assert_eq!(node.max_numeric_value(), Some(72.0));
    assert_eq!(node.numeric_value_step(), Some(1.0));
}

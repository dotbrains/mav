use super::super::*;
use crate::{AnyWindowHandle, AppContext as _, Context, InputEvent, Keystroke, TestAppContext};

/// Two focusable, clickable elements ("a" and "b") used to exercise the
/// Enter/Space -> synthesized click press/release pairing.
struct KeyboardActivationTest {
    focus_a: FocusHandle,
    focus_b: FocusHandle,
    clicks: Rc<RefCell<Vec<&'static str>>>,
}

impl Render for KeyboardActivationTest {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let clicks_a = self.clicks.clone();
        let clicks_b = self.clicks.clone();
        div()
            .size_full()
            .child(
                div()
                    .id("a")
                    .w(px(50.))
                    .h(px(50.))
                    .track_focus(&self.focus_a)
                    .on_click(move |_, _, _| clicks_a.borrow_mut().push("a")),
            )
            .child(
                div()
                    .id("b")
                    .w(px(50.))
                    .h(px(50.))
                    .track_focus(&self.focus_b)
                    .on_click(move |_, _, _| clicks_b.borrow_mut().push("b")),
            )
    }
}

fn setup_keyboard_activation_test() -> (
    TestAppContext,
    AnyWindowHandle,
    Rc<RefCell<Vec<&'static str>>>,
    FocusHandle,
    FocusHandle,
) {
    let mut cx = TestAppContext::single();
    let (focus_a, focus_b) = cx.update(|cx| (cx.focus_handle(), cx.focus_handle()));
    let clicks: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let window = cx.add_window({
        let focus_a = focus_a.clone();
        let focus_b = focus_b.clone();
        let clicks = clicks.clone();
        move |_, _| KeyboardActivationTest {
            focus_a,
            focus_b,
            clicks,
        }
    });
    (cx, window.into(), clicks, focus_a, focus_b)
}

/// Move focus to `handle`, flush effects, then paint so the newly focused
/// element registers its key handlers for the next dispatched event.
fn focus_and_draw(cx: &mut TestAppContext, window: AnyWindowHandle, handle: &FocusHandle) {
    cx.update_window(window, |_, window, cx| window.focus(handle, cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.draw(cx).clear();
    })
    .unwrap();
}

fn key_down(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(
            KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
}

fn key_up(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
    })
    .unwrap();
}

/// Pressing and releasing Enter on the same focused element fires a click.
#[test]
fn keyboard_activation_fires_click_on_same_element() {
    let (mut cx, window, clicks, focus_a, _focus_b) = setup_keyboard_activation_test();

    focus_and_draw(&mut cx, window, &focus_a);
    key_down(&mut cx, window, "enter");
    key_up(&mut cx, window, "enter");

    assert_eq!(*clicks.borrow(), vec!["a"]);
}

/// A key-down whose key-up lands on a *different* element (because focus
/// moved in between) must not leak a synthesized click onto the newly
/// focused element. This is the core regression: previously the key-up
/// handler fired unconditionally on whatever was focused at key-up time.
#[test]
fn keyboard_activation_does_not_leak_across_focus_change() {
    let (mut cx, window, clicks, focus_a, focus_b) = setup_keyboard_activation_test();

    // Enter pressed while "a" is focused...
    focus_and_draw(&mut cx, window, &focus_a);
    key_down(&mut cx, window, "enter");

    // ...focus moves to "b" before the release (as a confirm action would)...
    focus_and_draw(&mut cx, window, &focus_b);
    key_up(&mut cx, window, "enter");

    // ...so neither element is clicked: "a" never saw the up, and "b"
    // never saw the down.
    assert!(clicks.borrow().is_empty(), "clicks: {:?}", clicks.borrow());
}

/// A keydown whose flag is left pending because focus moved away before
/// the keyup must not fire a click when focus later *returns* to the same
/// element (the menu trigger reopening case). The stamped focus generation
/// no longer matches, so the stale pending state is ignored.
#[test]
fn keyboard_activation_does_not_leak_when_focus_returns() {
    let (mut cx, window, clicks, focus_a, focus_b) = setup_keyboard_activation_test();

    // Enter pressed on "a"...
    focus_and_draw(&mut cx, window, &focus_a);
    key_down(&mut cx, window, "enter");

    // ...focus leaves "a" before its keyup (so the pending state is never
    // consumed), then comes back to "a"...
    focus_and_draw(&mut cx, window, &focus_b);
    focus_and_draw(&mut cx, window, &focus_a);
    key_up(&mut cx, window, "enter");

    // ...and the now-stale pending keydown must not fire a click.
    assert!(clicks.borrow().is_empty(), "clicks: {:?}", clicks.borrow());
}

/// A non-activation key *released* during the press must cancel the pending
/// activation. For the sequence escape-down, space-down, escape-up,
/// space-up the space forms a clean down/up pair, but the intervening
/// escape-up means this isn't a plain space activation, so no click fires.
#[test]
fn keyboard_activation_cleared_by_intervening_key_release() {
    let (mut cx, window, clicks, focus_a, _focus_b) = setup_keyboard_activation_test();

    focus_and_draw(&mut cx, window, &focus_a);
    key_down(&mut cx, window, "escape");
    key_down(&mut cx, window, "space");
    key_up(&mut cx, window, "escape");
    key_up(&mut cx, window, "space");

    assert!(clicks.borrow().is_empty(), "clicks: {:?}", clicks.borrow());
}

/// The flag is a single activation marker, not keyed by which activation
/// key was used, so a Space down paired with an Enter up on the same
/// element still fires a click.
#[test]
fn keyboard_activation_does_not_distinguish_space_and_enter() {
    let (mut cx, window, clicks, focus_a, _focus_b) = setup_keyboard_activation_test();

    focus_and_draw(&mut cx, window, &focus_a);
    key_down(&mut cx, window, "space");
    key_up(&mut cx, window, "enter");

    assert_eq!(*clicks.borrow(), vec!["a"]);
}

/// A non-activation key pressed between the activation down and up clears
/// the pending flag, suppressing the click.
#[test]
fn keyboard_activation_cleared_by_intervening_keydown() {
    let (mut cx, window, clicks, focus_a, _focus_b) = setup_keyboard_activation_test();

    focus_and_draw(&mut cx, window, &focus_a);
    key_down(&mut cx, window, "enter");
    key_down(&mut cx, window, "a");
    key_up(&mut cx, window, "enter");

    assert!(clicks.borrow().is_empty(), "clicks: {:?}", clicks.borrow());
}

/// A modified Enter (e.g. cmd-enter) is not treated as an activation key,
/// so it neither sets the pending flag nor fires a click on release.
#[test]
fn keyboard_activation_ignores_modified_keys() {
    let (mut cx, window, clicks, focus_a, _focus_b) = setup_keyboard_activation_test();

    focus_and_draw(&mut cx, window, &focus_a);
    key_down(&mut cx, window, "cmd-enter");
    key_up(&mut cx, window, "cmd-enter");

    assert!(clicks.borrow().is_empty(), "clicks: {:?}", clicks.borrow());
}

use super::*;
use crate::{FocusHandle, Focusable, div, prelude::*};

struct Counter {
    count: usize,
    focus_handle: FocusHandle,
}

impl Counter {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        Self {
            count: 0,
            focus_handle,
        }
    }

    fn increment(&mut self, _cx: &mut Context<Self>) {
        self.count += 1;
    }
}

impl Focusable for Counter {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Counter {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(format!("Count: {}", self.count))
    }
}

#[test]
fn test_basic_usage() {
    let mut app = TestApp::new();

    let mut window = app.open_window(Counter::new);

    window.update(|counter, _window, cx| {
        counter.increment(cx);
    });

    window.read(|counter, _| {
        assert_eq!(counter.count, 1);
    });

    drop(window);
    app.update(|cx| cx.shutdown());
}

#[test]
fn test_entity_creation() {
    let mut app = TestApp::new();

    let entity = app.new_entity(|cx| Counter {
        count: 42,
        focus_handle: cx.focus_handle(),
    });

    app.read_entity(&entity, |counter, _| {
        assert_eq!(counter.count, 42);
    });

    app.update_entity(&entity, |counter, _cx| {
        counter.count += 1;
    });

    app.read_entity(&entity, |counter, _| {
        assert_eq!(counter.count, 43);
    });
}

#[test]
fn test_globals() {
    let mut app = TestApp::new();

    struct MyGlobal(String);
    impl Global for MyGlobal {}

    assert!(!app.has_global::<MyGlobal>());

    app.set_global(MyGlobal("hello".into()));

    assert!(app.has_global::<MyGlobal>());

    app.read_global::<MyGlobal, _>(|global, _| {
        assert_eq!(global.0, "hello");
    });

    app.update_global::<MyGlobal, _>(|global, _| {
        global.0 = "world".into();
    });

    app.read_global::<MyGlobal, _>(|global, _| {
        assert_eq!(global.0, "world");
    });
}

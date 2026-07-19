#![cfg_attr(target_family = "wasm", no_main)]
//! Example demonstrating GPUI's testing infrastructure.
//!
//! When run normally, this displays an interactive counter window.
//! The tests below demonstrate various GPUI testing patterns.
//!
//! Run the app: cargo run -p gpui --example testing
//! Run tests:   cargo test -p gpui --example testing --features test-support

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, Render, Task, Window, WindowBounds,
    WindowOptions, actions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

actions!(counter, [Increment, Decrement]);

struct Counter {
    count: i32,
    focus_handle: FocusHandle,
    _subscription: gpui::Subscription,
}

/// Event emitted by Counter
struct CounterEvent;

impl gpui::EventEmitter<CounterEvent> for Counter {}

impl Counter {
    fn new(cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe_self(|this: &mut Self, _event: &CounterEvent, _cx| {
            this.count = 999;
        });

        Self {
            count: 0,
            focus_handle: cx.focus_handle(),
            _subscription: subscription,
        }
    }

    fn increment(&mut self, _: &Increment, _window: &mut Window, cx: &mut Context<Self>) {
        self.count += 1;
        cx.notify();
    }

    fn decrement(&mut self, _: &Decrement, _window: &mut Window, cx: &mut Context<Self>) {
        self.count -= 1;
        cx.notify();
    }

    fn load(&self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            // Simulate loading data (e.g., from disk or network)
            this.update(cx, |counter, _| {
                counter.count = 100;
            })
            .ok();
        })
    }

    fn reload(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            // Simulate reloading data in the background
            this.update(cx, |counter, _| {
                counter.count += 50;
            })
            .ok();
        })
        .detach();
    }
}

impl Focusable for Counter {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Counter {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("counter")
            .key_context("Counter")
            .on_action(cx.listener(Self::increment))
            .on_action(cx.listener(Self::decrement))
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .gap_4()
            .bg(rgb(0x1e1e2e))
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .text_3xl()
                    .text_color(rgb(0xcdd6f4))
                    .child(format!("{}", self.count)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .id("decrement")
                            .px_4()
                            .py_2()
                            .bg(rgb(0x313244))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xcdd6f4))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.decrement(&Decrement, window, cx)
                            }))
                            .child("−"),
                    )
                    .child(
                        div()
                            .id("increment")
                            .px_4()
                            .py_2()
                            .bg(rgb(0x313244))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xcdd6f4))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.increment(&Increment, window, cx)
                            }))
                            .child("+"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .id("load")
                            .px_4()
                            .py_2()
                            .bg(rgb(0x313244))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xcdd6f4))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.load(cx).detach();
                            }))
                            .child("Load"),
                    )
                    .child(
                        div()
                            .id("reload")
                            .px_4()
                            .py_2()
                            .bg(rgb(0x313244))
                            .hover(|s| s.bg(rgb(0x45475a)))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xcdd6f4))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reload(cx);
                            }))
                            .child("Reload"),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0x6c7086))
                    .child("Press ↑/↓ or click buttons"),
            )
    }
}

fn run_example() {
    application().run(|cx: &mut App| {
        cx.bind_keys([
            gpui::KeyBinding::new("up", Increment, Some("Counter")),
            gpui::KeyBinding::new("down", Decrement, Some("Counter")),
        ]);

        let bounds = Bounds::centered(None, size(px(300.), px(200.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let counter = cx.new(|cx| Counter::new(cx));
                counter.focus_handle(cx).focus(window, cx);
                counter
            },
        )
        .unwrap();
    });
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    run_example();
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    gpui_platform::web_init();
    run_example();
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext};
    use rand::prelude::*;

    /// Here's a basic GPUI test. Just add the macro and take a TestAppContext as an argument!
    ///
    /// Note that synchronous side effects run immediately after your "update*" calls complete.
    #[gpui::test]
    fn basic_testing(cx: &mut TestAppContext) {
        let counter = cx.new(|cx| Counter::new(cx));

        counter.update(cx, |counter, _| {
            counter.count = 42;
        });

        // Note that TestAppContext doesn't support `read(cx)`
        let updated = counter.read_with(cx, |counter, _| counter.count);
        assert_eq!(updated, 42);

        // Emit an event - the subscriber will run immediately after the update finishes
        counter.update(cx, |_, cx| {
            cx.emit(CounterEvent);
        });

        let count_after_update = counter.read_with(cx, |counter, _| counter.count);
        assert_eq!(
            count_after_update, 999,
            "Side effects should run after update completes"
        );
    }

    /// Tests which involve the window require you to construct a VisualTestContext.
    /// Just like synchronous side effects, the window will be drawn after every "update*"
    /// call, so you can test render-dependent behavior.
    #[gpui::test]
    fn test_counter_in_window(cx: &mut TestAppContext) {
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |_, cx| cx.new(|cx| Counter::new(cx)))
                .unwrap()
        });

        let mut cx = VisualTestContext::from_window(window.into(), cx);
        let counter = window.root(&mut cx).unwrap();

        // Action dispatch depends on the element tree to resolve which action handler
        // to call, and this works exactly as you'd expect in a test.
        let focus_handle = counter.read_with(&cx, |counter, _| counter.focus_handle.clone());
        cx.update(|window, cx| {
            focus_handle.dispatch_action(&Increment, window, cx);
        });

        let count_after = counter.read_with(&cx, |counter, _| counter.count);
        assert_eq!(
            count_after, 1,
            "Action dispatched via focus handle should increment"
        );
    }

    /// GPUI tests can also be async, simply add the async keyword before the test.
    /// Note that the test executor is single thread, so async side effects (including
    /// background tasks) won't run until you explicitly yield control.
    #[gpui::test]
    async fn test_async_operations(cx: &mut TestAppContext) {
        let counter = cx.new(|cx| Counter::new(cx));

        // Tasks can be awaited directly
        counter.update(cx, |counter, cx| counter.load(cx)).await;

        let count = counter.read_with(cx, |counter, _| counter.count);
        assert_eq!(count, 100, "Load task should have set count to 100");

        // But side effects don't run until you yield control
        counter.update(cx, |counter, cx| counter.reload(cx));

        let count = counter.read_with(cx, |counter, _| counter.count);
        assert_eq!(count, 100, "Detached reload task shouldn't have run yet");

        // This runs all pending tasks
        cx.run_until_parked();

        let count = counter.read_with(cx, |counter, _| counter.count);
        assert_eq!(count, 150, "Reload task should have run after parking");
    }

    /// Note that the test executor panics if you await a future that waits on
    /// something outside GPUI's control, like a reading a file or network IO.
    /// You should mock external systems where possible, as this feature can be used
    /// to detect potential deadlocks in your async code.
    ///
    /// However, if you want to disable this check use `allow_parking()`
    #[gpui::test]
    async fn test_allow_parking(cx: &mut TestAppContext) {
        // Allow the thread to park
        cx.executor().allow_parking();

        // Simulate an external system (like a file system) with an OS thread
        let (tx, rx) = futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(5));
            tx.send(42).ok();
        });

        // Without allow_parking(), this await would panic because GPUI's
        // scheduler runs out of tasks while waiting for the external thread.
        let result = rx.await.unwrap();
        assert_eq!(result, 42);
    }

    /// GPUI also provides support for property testing, via the iterations flag
    #[gpui::test(iterations = 10)]
    fn test_counter_random_operations(cx: &mut TestAppContext, mut rng: StdRng) {
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |_, cx| cx.new(|cx| Counter::new(cx)))
                .unwrap()
        });
        let mut cx = VisualTestContext::from_window(window.into(), cx);

        let counter = cx.new(|cx| Counter::new(cx));

        // Perform random increments/decrements
        let mut expected = 0i32;
        for _ in 0..100 {
            if rng.random_bool(0.5) {
                expected += 1;
                counter.update_in(&mut cx, |counter, window, cx| {
                    counter.increment(&Increment, window, cx)
                });
            } else {
                expected -= 1;
                counter.update_in(&mut cx, |counter, window, cx| {
                    counter.decrement(&Decrement, window, cx)
                });
            }
        }

        let actual = counter.read_with(&cx, |counter, _| counter.count);
        assert_eq!(
            actual, expected,
            "Counter should match expected after random ops"
        );
    }

    /// Now, all of those tests are good, but GPUI also provides strong support for testing distributed systems.
    /// Let's setup a mock network and enhance the counter to send messages over it.
    mod distributed_systems {
        include!("testing/distributed_systems.rs");
    }
}

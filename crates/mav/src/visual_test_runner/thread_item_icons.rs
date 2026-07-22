use super::*;

#[cfg(target_os = "macos")]
struct ThreadItemIconDecorationsTestView;

#[cfg(target_os = "macos")]
impl gpui::Render for ThreadItemIconDecorationsTestView {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use ui::{IconName, Label, LabelSize, ThreadItem, prelude::*};

        let section_label = |text: &str| {
            Label::new(text.to_string())
                .size(LabelSize::Small)
                .color(Color::Muted)
        };

        let container = || {
            v_flex()
                .w_80()
                .border_1()
                .border_color(cx.theme().colors().border_variant)
                .bg(cx.theme().colors().panel_background)
        };

        v_flex()
            .size_full()
            .bg(cx.theme().colors().background)
            .p_4()
            .gap_3()
            .child(
                Label::new("ThreadItem Icon Decorations")
                    .size(LabelSize::Large)
                    .color(Color::Default),
            )
            .child(section_label("No decoration (default idle)"))
            .child(
                container()
                    .child(ThreadItem::new("ti-none", "Default idle thread").timestamp("1:00 AM")),
            )
            .child(section_label("Blue dot (notified)"))
            .child(
                container().child(
                    ThreadItem::new("ti-done", "Generation completed successfully")
                        .timestamp("1:05 AM")
                        .notified(true),
                ),
            )
            .child(section_label("Yellow triangle (waiting for confirmation)"))
            .child(
                container().child(
                    ThreadItem::new("ti-waiting", "Waiting for user confirmation")
                        .timestamp("1:10 AM")
                        .status(ui::AgentThreadStatus::WaitingForConfirmation),
                ),
            )
            .child(section_label("Red X (error)"))
            .child(
                container().child(
                    ThreadItem::new("ti-error", "Failed to connect to server")
                        .timestamp("1:15 AM")
                        .status(ui::AgentThreadStatus::Error),
                ),
            )
            .child(section_label("Spinner (running)"))
            .child(
                container().child(
                    ThreadItem::new("ti-running", "Generating response...")
                        .icon(IconName::AiClaude)
                        .timestamp("1:20 AM")
                        .status(ui::AgentThreadStatus::Running),
                ),
            )
            .child(section_label(
                "Spinner + yellow triangle (waiting for confirmation)",
            ))
            .child(
                container().child(
                    ThreadItem::new("ti-running-waiting", "Running but needs confirmation")
                        .icon(IconName::AiClaude)
                        .timestamp("1:25 AM")
                        .status(ui::AgentThreadStatus::WaitingForConfirmation),
                ),
            )
    }
}

#[cfg(target_os = "macos")]
fn run_thread_item_icon_decorations_visual_tests(
    _app_state: Arc<AppState>,
    cx: &mut VisualTestAppContext,
    update_baseline: bool,
) -> Result<TestResult> {
    let window_size = size(px(400.0), px(600.0));
    let bounds = Bounds {
        origin: point(px(0.0), px(0.0)),
        size: window_size,
    };

    let window = cx
        .update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    focus: false,
                    show: false,
                    ..Default::default()
                },
                |_window, cx| cx.new(|_| ThreadItemIconDecorationsTestView),
            )
        })
        .context("Failed to open thread item icon decorations test window")?;

    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, _cx| {
        window.refresh();
    })?;

    cx.run_until_parked();

    let test_result = run_visual_test(
        "thread_item_icon_decorations",
        window.into(),
        cx,
        update_baseline,
    )?;

    cx.update_window(window.into(), |_, window, _cx| {
        window.remove_window();
    })
    .log_err();

    cx.run_until_parked();

    for _ in 0..15 {
        cx.advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
    }

    Ok(test_result)
}

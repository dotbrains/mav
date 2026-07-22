use super::*;

#[cfg(target_os = "macos")]
struct ErrorWrappingTestView;

#[cfg(target_os = "macos")]
impl gpui::Render for ErrorWrappingTestView {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use ui::{Button, Callout, IconName, LabelSize, Severity, prelude::*, v_flex};

        let long_error_message = "Rate limit reached for gpt-5.2-codex in organization \
            org-QmYpir6k6dkULKU1XUSN6pal on tokens per min (TPM): Limit 500000, Used 442480, \
            Requested 59724. Please try again in 264ms. Visit \
            https://platform.openai.com/account/rate-limits to learn more.";

        let retry_description = "Retrying. Next attempt in 4 seconds (Attempt 1 of 2).";

        v_flex()
            .size_full()
            .bg(cx.theme().colors().background)
            .p_4()
            .gap_4()
            .child(
                Callout::new()
                    .icon(IconName::Warning)
                    .severity(Severity::Warning)
                    .title(long_error_message)
                    .description(retry_description),
            )
            .child(
                Callout::new()
                    .severity(Severity::Error)
                    .icon(IconName::XCircle)
                    .title("An Error Happened")
                    .description(long_error_message)
                    .actions_slot(Button::new("dismiss", "Dismiss").label_size(LabelSize::Small)),
            )
            .child(
                Callout::new()
                    .severity(Severity::Error)
                    .icon(IconName::XCircle)
                    .title(long_error_message)
                    .actions_slot(Button::new("retry", "Retry").label_size(LabelSize::Small)),
            )
    }
}

#[cfg(target_os = "macos")]
fn run_error_wrapping_visual_tests(
    _app_state: Arc<AppState>,
    cx: &mut VisualTestAppContext,
    update_baseline: bool,
) -> Result<TestResult> {
    let window_size = size(px(500.0), px(400.0));
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
                |_window, cx| cx.new(|_| ErrorWrappingTestView),
            )
        })
        .context("Failed to open error wrapping test window")?;

    cx.run_until_parked();

    cx.update_window(window.into(), |_, window, _cx| {
        window.refresh();
    })?;

    cx.run_until_parked();

    let test_result =
        run_visual_test("error_message_wrapping", window.into(), cx, update_baseline)?;

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

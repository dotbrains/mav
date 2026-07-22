use super::*;

use super::super::*;
use gpui::{
    Entity, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualContext, VisualTestContext, point,
};
use util::default;
use util_macros::perf;

async fn init_scroll_perf_test(
    cx: &mut TestAppContext,
) -> (Entity<Terminal>, &mut VisualTestContext) {
    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
    });

    cx.executor().allow_parking();

    let window = cx.add_empty_window();
    let builder = window
        .update(|window, cx| {
            let settings = TerminalSettings::get_global(cx);
            let test_path_hyperlink_timeout_ms = 100;
            TerminalBuilder::new(
                None,
                None,
                task::Shell::System,
                HashMap::default(),
                SettingsCursorShape::default(),
                AlternateScroll::On,
                None,
                settings.path_hyperlink_regexes.clone(),
                test_path_hyperlink_timeout_ms,
                false,
                window.window_handle().window_id().as_u64(),
                None,
                cx,
                vec![],
                PathStyle::local(),
            )
        })
        .await
        .unwrap();
    let terminal = window.new(|cx| builder.subscribe(cx));

    terminal.update(window, |term, cx| {
        term.write_output("long line ".repeat(1000).as_bytes(), cx);
    });

    (terminal, window)
}

#[perf]
#[gpui::test]
async fn scroll_long_line_benchmark(cx: &mut TestAppContext) {
    let (terminal, window) = init_scroll_perf_test(cx).await;
    let wobble = point(FIND_HYPERLINK_THROTTLE_PX, px(0.0));
    let mut scroll_by = |lines: i32| {
        window.update_window_entity(&terminal, |terminal, window, cx| {
            let bounds = terminal.last_content.terminal_bounds.bounds;
            let center = bounds.origin + bounds.center();
            let position = center + wobble * lines as f32;

            terminal.mouse_move(
                &MouseMoveEvent {
                    position,
                    ..default()
                },
                cx,
            );

            terminal.scroll_wheel(
                &ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Lines(GpuiPoint::new(0.0, lines as f32)),
                    ..default()
                },
                1.0,
            );

            assert!(
                terminal
                    .events
                    .iter()
                    .any(|event| matches!(event, InternalEvent::Scroll(_))),
                "Should have Scroll event when scrolling within terminal bounds"
            );
            terminal.sync(window, cx);
        });
    };

    for _ in 0..20000 {
        scroll_by(1);
        scroll_by(-1);
    }
}

#[test]
fn test_num_lines_float_precision() {
    let line_heights = [
        20.1f32, 16.7, 18.3, 22.9, 14.1, 15.6, 17.8, 19.4, 21.3, 23.7,
    ];
    for &line_height in &line_heights {
        for n in 1..=100 {
            let height = n as f32 * line_height;
            let bounds = TerminalBounds::new(
                px(line_height),
                px(8.0),
                Bounds {
                    origin: GpuiPoint::default(),
                    size: gpui::Size {
                        width: px(800.0),
                        height: px(height),
                    },
                },
            );
            assert_eq!(
                bounds.num_lines(),
                n,
                "num_lines() should be {n} for height={height}, line_height={line_height}"
            );
        }
    }
}

#[test]
fn test_num_columns_float_precision() {
    let cell_widths = [8.1f32, 7.3, 9.7, 6.9, 10.1];
    for &cell_width in &cell_widths {
        for n in 1..=200 {
            let width = n as f32 * cell_width;
            let bounds = TerminalBounds::new(
                px(20.0),
                px(cell_width),
                Bounds {
                    origin: GpuiPoint::default(),
                    size: gpui::Size {
                        width: px(width),
                        height: px(400.0),
                    },
                },
            );
            assert_eq!(
                bounds.num_columns(),
                n,
                "num_columns() should be {n} for width={width}, cell_width={cell_width}"
            );
        }
    }
}

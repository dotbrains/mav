use super::*;

fn init_ctrl_click_hyperlink_test(cx: &mut TestAppContext, output: &[u8]) -> Entity<Terminal> {
    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
    });

    let terminal = cx.new(|cx| {
        TerminalBuilder::new_display_only(
            SettingsCursorShape::default(),
            AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        )
        .subscribe(cx)
    });

    terminal.update(cx, |terminal, cx| {
        terminal.write_output(output, cx);
    });

    cx.run_until_parked();

    terminal.update(cx, |terminal, _cx| {
        let term_lock = terminal.term.lock();
        terminal.last_content = make_content(&term_lock, &terminal.last_content);
        drop(term_lock);

        let terminal_bounds = TerminalBounds::new(
            px(20.0),
            px(10.0),
            bounds(point(px(0.0), px(0.0)), size(px(400.0), px(400.0))),
        );
        terminal.last_content.terminal_bounds = terminal_bounds;
        terminal.events.clear();
    });

    terminal
}

fn ctrl_mouse_down_at(
    terminal: &mut Terminal,
    position: GpuiPoint<Pixels>,
    cx: &mut Context<Terminal>,
) {
    let mouse_down = MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::secondary_key(),
        click_count: 1,
        first_mouse: true,
    };
    terminal.mouse_down(&mouse_down, cx);
}

fn ctrl_mouse_move_to(
    terminal: &mut Terminal,
    position: GpuiPoint<Pixels>,
    cx: &mut Context<Terminal>,
) {
    let terminal_bounds = terminal.last_content.terminal_bounds.bounds;
    let drag_event = MouseMoveEvent {
        position,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::secondary_key(),
    };
    terminal.mouse_drag(&drag_event, terminal_bounds, cx);
}

fn ctrl_mouse_up_at(
    terminal: &mut Terminal,
    position: GpuiPoint<Pixels>,
    cx: &mut Context<Terminal>,
) {
    let mouse_up = MouseUpEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::secondary_key(),
        click_count: 1,
    };
    terminal.mouse_up(&mouse_up, cx);
}

fn left_mouse_down_at(
    terminal: &mut Terminal,
    position: GpuiPoint<Pixels>,
    cx: &mut Context<Terminal>,
) {
    let mouse_down = MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: true,
    };
    terminal.mouse_down(&mouse_down, cx);
}

fn left_mouse_drag_to(
    terminal: &mut Terminal,
    position: GpuiPoint<Pixels>,
    cx: &mut Context<Terminal>,
) {
    let region = terminal.last_content.terminal_bounds.bounds;
    let drag_event = MouseMoveEvent {
        position,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    };
    terminal.mouse_drag(&drag_event, region, cx);
}

/// A left click that jitters by a pixel or two (e.g. the window-focusing
/// click) must not begin a selection, otherwise `copy_on_select` would
/// overwrite the clipboard. Regression test for #58970.
#[gpui::test]
async fn test_terminal_click_jitter_does_not_start_selection(cx: &mut TestAppContext) {
    let terminal = init_ctrl_click_hyperlink_test(cx, b"hello world\r\n");

    terminal.update(cx, |terminal, cx| {
        left_mouse_down_at(terminal, point(px(50.0), px(10.0)), cx);
        terminal.events.clear();

        // One pixel of movement is below the drag threshold.
        left_mouse_drag_to(terminal, point(px(51.0), px(10.0)), cx);

        assert!(
            !terminal
                .events
                .iter()
                .any(|event| matches!(event, InternalEvent::UpdateSelection(_))),
            "a sub-threshold click jitter should not start a selection"
        );
        assert!(terminal.selection_phase == SelectionPhase::Ended);
    });
}

/// A deliberate drag past the threshold must still start a selection.
#[gpui::test]
async fn test_terminal_deliberate_drag_starts_selection(cx: &mut TestAppContext) {
    let terminal = init_ctrl_click_hyperlink_test(cx, b"hello world\r\n");

    terminal.update(cx, |terminal, cx| {
        left_mouse_down_at(terminal, point(px(50.0), px(10.0)), cx);
        terminal.events.clear();

        // Well beyond the drag threshold.
        left_mouse_drag_to(terminal, point(px(90.0), px(10.0)), cx);

        assert!(
            terminal
                .events
                .iter()
                .any(|event| matches!(event, InternalEvent::UpdateSelection(_))),
            "a deliberate drag should start a selection"
        );
        assert!(terminal.selection_phase == SelectionPhase::Selecting);
    });
}

#[gpui::test]
async fn test_hyperlink_ctrl_click_same_position(cx: &mut TestAppContext) {
    let terminal = init_ctrl_click_hyperlink_test(cx, b"Visit https://mav.dev/ for more\r\n");

    terminal.update(cx, |terminal, cx| {
        let click_position = point(px(80.0), px(10.0));
        ctrl_mouse_down_at(terminal, click_position, cx);
        ctrl_mouse_up_at(terminal, click_position, cx);

        assert!(
            terminal
                .events
                .iter()
                .any(|event| matches!(event, InternalEvent::ProcessHyperlink(_, true))),
            "Should have ProcessHyperlink event when ctrl+clicking on same hyperlink position"
        );
    });
}

#[gpui::test]
async fn test_hyperlink_ctrl_click_drag_outside_bounds(cx: &mut TestAppContext) {
    let terminal = init_ctrl_click_hyperlink_test(
        cx,
        b"Visit https://mav.dev/ for more\r\nThis is another line\r\n",
    );

    terminal.update(cx, |terminal, cx| {
        let down_position = point(px(80.0), px(10.0));
        let up_position = point(px(10.0), px(50.0));

        ctrl_mouse_down_at(terminal, down_position, cx);
        ctrl_mouse_move_to(terminal, up_position, cx);
        ctrl_mouse_up_at(terminal, up_position, cx);

        assert!(
            !terminal
                .events
                .iter()
                .any(|event| matches!(event, InternalEvent::ProcessHyperlink(_, _))),
            "Should NOT have ProcessHyperlink event when dragging outside the hyperlink"
        );
    });
}

#[gpui::test]
async fn test_hyperlink_ctrl_click_drag_within_bounds(cx: &mut TestAppContext) {
    let terminal = init_ctrl_click_hyperlink_test(cx, b"Visit https://mav.dev/ for more\r\n");

    terminal.update(cx, |terminal, cx| {
        let down_position = point(px(70.0), px(10.0));
        let up_position = point(px(130.0), px(10.0));

        ctrl_mouse_down_at(terminal, down_position, cx);
        ctrl_mouse_move_to(terminal, up_position, cx);
        ctrl_mouse_up_at(terminal, up_position, cx);

        assert!(
            terminal
                .events
                .iter()
                .any(|event| matches!(event, InternalEvent::ProcessHyperlink(_, true))),
            "Should have ProcessHyperlink event when dragging within hyperlink bounds"
        );
    });
}

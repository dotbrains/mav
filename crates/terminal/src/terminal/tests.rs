use std::time::Duration;

use super::*;
use crate::{Cell, Content, IndexedCell, TerminalBounds, TerminalBuilder, content_index_for_mouse};
use async_channel::Receiver;
use collections::HashMap;
use gpui::MouseMoveEvent;
use gpui::{
    ClipboardItem, Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    TestAppContext, bounds, point, size,
};
use parking_lot::Mutex;
use rand::{Rng, distr, rngs::StdRng};
use task::{Shell, ShellBuilder};
#[path = "../tests/mouse.rs"]
mod mouse;
#[path = "../tests/startup.rs"]
mod startup;

#[cfg(not(target_os = "windows"))]
fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });
}

/// Helper to build a test terminal running a shell command.
/// Returns the terminal entity and a receiver for the completion signal.
async fn build_test_terminal(
    cx: &mut TestAppContext,
    command: &str,
    args: &[&str],
) -> (Entity<Terminal>, Receiver<Option<ExitStatus>>) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (program, args) =
        ShellBuilder::new(&Shell::System, false).build(Some(command.to_owned()), &args);
    build_test_terminal_with_arguments(cx, program, args).await
}

async fn build_test_terminal_with_arguments(
    cx: &mut TestAppContext,
    program: String,
    args: Vec<String>,
) -> (Entity<Terminal>, Receiver<Option<ExitStatus>>) {
    let (completion_tx, completion_rx) = async_channel::unbounded();
    let builder = cx
        .update(|cx| {
            TerminalBuilder::new(
                None,
                None,
                task::Shell::WithArguments {
                    program,
                    args,
                    title_override: None,
                },
                HashMap::default(),
                SettingsCursorShape::default(),
                AlternateScroll::On,
                None,
                vec![],
                0,
                false,
                0,
                Some(completion_tx),
                cx,
                vec![],
                PathStyle::local(),
            )
        })
        .await
        .unwrap();
    let terminal = cx.new(|cx| builder.subscribe(cx));
    (terminal, completion_rx)
}

async fn assert_content_eventually(
    terminal: &Entity<Terminal>,
    expected: &str,
    cx: &mut TestAppContext,
) {
    let mut content = String::new();
    for _ in 0..100 {
        content = terminal.update(cx, |term, _| term.get_content());
        if content.contains(expected) {
            return;
        }
        cx.background_executor
            .timer(Duration::from_millis(10))
            .await;
    }
    panic!("Expected terminal content to contain {expected:?}, got: {content}");
}

#[cfg(unix)]
async fn assert_foreground_process_command_eventually(
    terminal: &Entity<Terminal>,
    expected: &str,
    cx: &mut TestAppContext,
) {
    let mut command_name = None;
    for _ in 0..100 {
        terminal.update(cx, |terminal, _| {
            if let TerminalType::Pty { info, .. } = &terminal.terminal_type {
                info.load_for_test();
            }
        });
        command_name =
            terminal.update(cx, |terminal, _| terminal.foreground_process_command_name());
        if command_name.as_deref() == Some(expected) {
            return;
        }
        cx.background_executor
            .timer(Duration::from_millis(10))
            .await;
    }
    let process_info = terminal.update(cx, |terminal, _| match &terminal.terminal_type {
        TerminalType::Pty { info, .. } => format!(
            "pid={:?}, fallback_pid={:?}, has_current_info={}",
            info.pid(),
            info.pid_getter().fallback_pid(),
            info.current.read().is_some()
        ),
        TerminalType::DisplayOnly => "display-only".to_string(),
    });
    panic!(
        "Expected foreground process command name to be {expected:?}, got {command_name:?}; process info: {process_info:?}"
    );
}

#[path = "tests_process.rs"]
mod process;

#[path = "tests_viewport.rs"]
mod viewport;

#[path = "tests_io.rs"]
mod io;

#[path = "tests_lifecycle.rs"]
mod lifecycle;

#[path = "tests_perf.rs"]
mod perf;

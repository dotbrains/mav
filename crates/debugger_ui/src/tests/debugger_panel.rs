#![expect(clippy::result_large_err)]
use crate::{
    persistence::DebuggerPaneItem,
    tests::{start_debug_session, start_debug_session_with},
    *,
};
use dap::{
    ErrorResponse, Message, RunInTerminalRequestArguments, SourceBreakpoint,
    StartDebuggingRequestArguments, StartDebuggingRequestArgumentsRequest,
    adapters::DebugTaskDefinition,
    client::SessionId,
    requests::{
        Continue, Disconnect, Launch, Next, RunInTerminal, SetBreakpoints, StackTrace,
        StartDebugging, StepBack, StepIn, StepOut, Threads,
    },
};
use editor::{
    ActiveDebugLine, Editor, EditorMode, MultiBuffer,
    actions::{self},
};
use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
use project::{
    FakeFs, Project,
    debugger::session::{ThreadId, ThreadStatus},
};
use serde_json::json;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use terminal_view::terminal_panel::TerminalPanel;
use tests::{active_debug_session_panel, init_test, init_test_workspace};
use util::{path, rel_path::rel_path};
use workspace::item::SaveOptions;
use workspace::pane_group::SplitDirection;
use workspace::{Item, dock::Panel, move_active_item};

mod active_line;
mod app_quit;
mod basic;
mod breakpoints;
mod config;
mod panel_status;
mod restart;
mod reverse_requests;
mod session_shutdown;
mod split_view;

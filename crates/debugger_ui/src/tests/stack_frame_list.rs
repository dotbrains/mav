#![expect(clippy::result_large_err)]
use crate::{
    debugger_panel::DebugPanel,
    session::running::stack_frame_list::{
        StackFrameEntry, StackFrameFilter, stack_frame_filter_key,
    },
    tests::{active_debug_session_panel, init_test, init_test_workspace, start_debug_session},
};
use dap::{
    StackFrame,
    requests::{Scopes, StackTrace, Threads},
};
use db::kvp::KeyValueStore;
use editor::{Editor, ToPoint as _};
use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
use project::{FakeFs, Project};
use serde_json::json;
use std::sync::Arc;
use unindent::Unindent as _;
use util::{path, rel_path::rel_path};
use workspace::Item;

mod collapsed_entries;
mod filter;
mod filter_persistence;
mod initial_frames;
mod select_frame;

#![expect(clippy::result_large_err)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{
    DebugPanel,
    persistence::DebuggerPaneItem,
    session::running::variable_list::{
        AddWatch, CollapseSelectedEntry, ExpandSelectedEntry, RemoveWatch,
    },
    tests::{active_debug_session_panel, init_test, init_test_workspace, start_debug_session},
};
use collections::HashMap;
use dap::{
    Scope, StackFrame, Variable,
    requests::{Evaluate, Initialize, Launch, Scopes, StackTrace, Variables},
};
use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
use menu::{SelectFirst, SelectNext, SelectPrevious};
use project::{FakeFs, Project};
use serde_json::json;
use ui::SharedString;
use unindent::Unindent as _;
use util::path;

mod basic;
mod keyboard_navigation;
mod multiple_scopes;
mod render_requests;
mod stack_frame_selection;
mod support;
mod watchers;

use support::*;

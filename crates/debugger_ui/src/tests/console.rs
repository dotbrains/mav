#![expect(clippy::result_large_err)]
use crate::{
    tests::{active_debug_session_panel, start_debug_session},
    *,
};
use dap::requests::StackTrace;
use editor::{DisplayPoint, display_map::DisplayRow};
use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
use project::{FakeFs, Project};
use serde_json::json;
use tests::{init_test, init_test_workspace};
use util::path;

mod disabled_evaluate_expression;
mod disabled_grouped_output;
mod escape_codes;
mod output_event;

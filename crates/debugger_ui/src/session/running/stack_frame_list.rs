use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use dap::StackFrameId;
use dap::adapters::DebugAdapterName;
use db::kvp::KeyValueStore;
use gpui::{
    Action, AnyElement, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, ListState,
    Subscription, Task, TaskExt, WeakEntity, list,
};
use util::{
    debug_panic,
    paths::{PathStyle, is_absolute},
};

use crate::ToggleUserFrames;
use language::PointUtf16;
use project::debugger::breakpoint_store::ActiveStackFrame;
use project::debugger::session::{Session, SessionEvent, StackFrame, ThreadStatus};
use project::{ProjectItem, ProjectPath};
use ui::{Tooltip, WithScrollbar, prelude::*};
use workspace::{Workspace, WorkspaceId};

use super::RunningState;

mod builder;
mod constructor;
mod navigation;
mod render;
mod root_render;

pub enum StackFrameListEvent {
    SelectedStackFrameChanged(StackFrameId),
    BuiltEntries,
}

/// Represents the filter applied to the stack frame list
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub(crate) enum StackFrameFilter {
    /// Show all frames
    All,
    /// Show only frames from the user's code
    OnlyUserFrames,
}

impl StackFrameFilter {
    fn from_str_or_default(s: impl AsRef<str>) -> Self {
        match s.as_ref() {
            "user" => StackFrameFilter::OnlyUserFrames,
            "all" => StackFrameFilter::All,
            _ => StackFrameFilter::All,
        }
    }
}

impl From<StackFrameFilter> for String {
    fn from(filter: StackFrameFilter) -> Self {
        match filter {
            StackFrameFilter::All => "all".to_string(),
            StackFrameFilter::OnlyUserFrames => "user".to_string(),
        }
    }
}

pub(crate) fn stack_frame_filter_key(
    adapter_name: &DebugAdapterName,
    workspace_id: WorkspaceId,
) -> String {
    let database_id: i64 = workspace_id.into();
    format!("stack-frame-list-filter-{}-{}", adapter_name.0, database_id)
}

pub struct StackFrameList {
    focus_handle: FocusHandle,
    _subscription: Subscription,
    session: Entity<Session>,
    state: WeakEntity<RunningState>,
    entries: Vec<StackFrameEntry>,
    workspace: WeakEntity<Workspace>,
    selected_ix: Option<usize>,
    opened_stack_frame_id: Option<StackFrameId>,
    list_state: ListState,
    list_filter: StackFrameFilter,
    filter_entries_indices: Vec<usize>,
    error: Option<SharedString>,
    _refresh_task: Task<()>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StackFrameEntry {
    Normal(dap::StackFrame),
    /// Used to indicate that the frame is artificial and is a visual label or separator
    Label(dap::StackFrame),
    Collapsed(Vec<dap::StackFrame>),
}

impl Focusable for StackFrameList {
    fn focus_handle(&self, _: &gpui::App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<StackFrameListEvent> for StackFrameList {}

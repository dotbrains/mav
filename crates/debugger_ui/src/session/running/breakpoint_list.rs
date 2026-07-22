use std::{
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use dap::{Capabilities, ExceptionBreakpointsFilter, adapters::DebugAdapterName};
use db::kvp::KeyValueStore;
use editor::Editor;
use gpui::{
    Action, AppContext, ClickEvent, Entity, FocusHandle, Focusable, MouseButton, ScrollStrategy,
    Task, UniformListScrollHandle, WeakEntity, actions, uniform_list,
};
use itertools::Itertools;
use language::Point;
use mav_actions::{ToggleEnableBreakpoint, UnsetBreakpoint};
use project::{
    Project,
    debugger::{
        breakpoint_store::{BreakpointEditAction, BreakpointStore, SourceBreakpoint},
        dap_store::{DapStore, PersistedAdapterOptions},
        session::Session,
    },
    worktree_store::WorktreeStore,
};
use ui::{
    Divider, DividerColor, FluentBuilder as _, Indicator, IntoElement, ListItem, Render,
    ScrollAxes, StatefulInteractiveElement, Tooltip, WithScrollbar, prelude::*,
};
use util::rel_path::RelPath;
use workspace::Workspace;

actions!(
    debugger,
    [
        /// Navigates to the previous breakpoint property in the list.
        PreviousBreakpointProperty,
        /// Navigates to the next breakpoint property in the list.
        NextBreakpointProperty
    ]
);

mod actions;
mod entry;
mod entry_render;
mod list_render;
mod root_render;
mod strip;

pub(crate) enum SelectedBreakpointKind {
    Source,
    Exception,
    Data,
}
pub(crate) struct BreakpointList {
    workspace: WeakEntity<Workspace>,
    breakpoint_store: Entity<BreakpointStore>,
    dap_store: Entity<DapStore>,
    worktree_store: Entity<WorktreeStore>,
    breakpoints: Vec<BreakpointEntry>,
    session: Option<Entity<Session>>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    selected_ix: Option<usize>,
    max_width_index: Option<usize>,
    input: Entity<Editor>,
    strip_mode: Option<ActiveBreakpointStripMode>,
    serialize_exception_breakpoints_task: Option<Task<anyhow::Result<()>>>,
}

impl Focusable for BreakpointList {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ActiveBreakpointStripMode {
    Log,
    Condition,
    HitCondition,
}

struct LineBreakpoint {
    name: SharedString,
    dir: Option<SharedString>,
    line: u32,
    breakpoint: SourceBreakpoint,
}

struct ExceptionBreakpoint {
    id: String,
    data: ExceptionBreakpointsFilter,
    is_enabled: bool,
}

#[derive(Clone, Debug)]
struct DataBreakpoint(project::debugger::session::DataBreakpointState);

enum BreakpointEntryKind {
    LineBreakpoint(LineBreakpoint),
    ExceptionBreakpoint(ExceptionBreakpoint),
    DataBreakpoint(DataBreakpoint),
}

#[derive(Clone, Debug)]
struct BreakpointEntry {
    kind: BreakpointEntryKind,
    weak: WeakEntity<BreakpointList>,
}

impl From<&Capabilities> for SupportedBreakpointProperties {
    fn from(caps: &Capabilities) -> Self {
        let mut this = Self::empty();
        for (prop, offset) in [
            (caps.supports_log_points, Self::LOG),
            (caps.supports_conditional_breakpoints, Self::CONDITION),
            (
                caps.supports_hit_conditional_breakpoints,
                Self::HIT_CONDITION,
            ),
            (
                caps.supports_exception_options,
                Self::EXCEPTION_FILTER_OPTIONS,
            ),
        ] {
            if prop.unwrap_or_default() {
                this.insert(offset);
            }
        }
        this
    }
}

impl SupportedBreakpointProperties {
    fn for_exception_breakpoints(self) -> Self {
        // TODO: we don't yet support conditions for exception breakpoints at the data layer, hence all props are disabled here.
        Self::empty()
    }
    fn for_data_breakpoints(self) -> Self {
        // TODO: we don't yet support conditions for data breakpoints at the data layer, hence all props are disabled here.
        Self::empty()
    }
}
#[derive(IntoElement)]
struct BreakpointOptionsStrip {
    props: SupportedBreakpointProperties,
    breakpoint: BreakpointEntry,
    is_selected: bool,
    focus_handle: FocusHandle,
    strip_mode: Option<ActiveBreakpointStripMode>,
    index: usize,
}

use crate::session::running::{RunningState, memory_view::MemoryView};

use super::stack_frame_list::{StackFrameList, StackFrameListEvent};
use dap::{
    ScopePresentationHint, StackFrameId, VariablePresentationHint, VariablePresentationHintKind,
    VariableReference,
};
use editor::Editor;
use gpui::{
    Action, AnyElement, ClickEvent, ClipboardItem, Context, DismissEvent, Empty, Entity,
    FocusHandle, Focusable, Hsla, MouseDownEvent, Point, Subscription, TaskExt,
    TextStyleRefinement, UniformListScrollHandle, WeakEntity, actions, anchored, deferred,
    uniform_list,
};
use itertools::Itertools;
use menu::{SelectFirst, SelectLast, SelectNext, SelectPrevious};
use project::debugger::{
    dap_command::DataBreakpointContext,
    session::{Session, SessionEvent, Watcher},
};
use std::{collections::HashMap, ops::Range, sync::Arc};
use ui::{ContextMenu, ListItem, ScrollAxes, ScrollableHandle, Tooltip, WithScrollbar, prelude::*};
use util::{debug_panic, maybe};

static INDENT_STEP_SIZE: Pixels = px(10.0);

mod colors;
mod constructor;
mod context_actions;
mod editor;
mod entries;
mod render;
mod root_render;
mod selection;
#[cfg(test)]
mod tests;
mod watchers;

actions!(
    variable_list,
    [
        /// Expands the selected variable entry to show its children.
        ExpandSelectedEntry,
        /// Collapses the selected variable entry to hide its children.
        CollapseSelectedEntry,
        /// Copies the variable name to the clipboard.
        CopyVariableName,
        /// Copies the variable value to the clipboard.
        CopyVariableValue,
        /// Edits the value of the selected variable.
        EditVariable,
        /// Adds the selected variable to the watch list.
        AddWatch,
        /// Removes the selected variable from the watch list.
        RemoveWatch,
        /// Jump to variable's memory location.
        GoToMemory,
    ]
);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct EntryState {
    depth: usize,
    is_expanded: bool,
    has_children: bool,
    parent_reference: VariableReference,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct EntryPath {
    pub leaf_name: Option<SharedString>,
    pub indices: Arc<[SharedString]>,
}

impl EntryPath {
    fn for_watcher(expression: impl Into<SharedString>) -> Self {
        Self {
            leaf_name: Some(expression.into()),
            indices: Arc::new([]),
        }
    }

    fn for_scope(scope_name: impl Into<SharedString>) -> Self {
        Self {
            leaf_name: Some(scope_name.into()),
            indices: Arc::new([]),
        }
    }

    fn with_name(&self, name: SharedString) -> Self {
        Self {
            leaf_name: Some(name),
            indices: self.indices.clone(),
        }
    }

    /// Create a new child of this variable path
    fn with_child(&self, name: SharedString) -> Self {
        Self {
            leaf_name: None,
            indices: self
                .indices
                .iter()
                .cloned()
                .chain(std::iter::once(name))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum DapEntry {
    Watcher(Watcher),
    Variable(dap::Variable),
    Scope(dap::Scope),
}

impl DapEntry {
    fn as_watcher(&self) -> Option<&Watcher> {
        match self {
            DapEntry::Watcher(watcher) => Some(watcher),
            _ => None,
        }
    }

    fn as_variable(&self) -> Option<&dap::Variable> {
        match self {
            DapEntry::Variable(dap) => Some(dap),
            _ => None,
        }
    }

    fn as_scope(&self) -> Option<&dap::Scope> {
        match self {
            DapEntry::Scope(dap) => Some(dap),
            _ => None,
        }
    }

    #[cfg(test)]
    fn name(&self) -> &str {
        match self {
            DapEntry::Watcher(watcher) => &watcher.expression,
            DapEntry::Variable(dap) => &dap.name,
            DapEntry::Scope(dap) => &dap.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ListEntry {
    entry: DapEntry,
    path: EntryPath,
}

impl ListEntry {
    fn as_watcher(&self) -> Option<&Watcher> {
        self.entry.as_watcher()
    }

    fn as_variable(&self) -> Option<&dap::Variable> {
        self.entry.as_variable()
    }

    fn as_scope(&self) -> Option<&dap::Scope> {
        self.entry.as_scope()
    }

    fn item_id(&self) -> ElementId {
        use std::fmt::Write;
        let mut id = match &self.entry {
            DapEntry::Watcher(watcher) => format!("watcher-{}", watcher.expression),
            DapEntry::Variable(dap) => format!("variable-{}", dap.name),
            DapEntry::Scope(dap) => format!("scope-{}", dap.name),
        };
        for name in self.path.indices.iter() {
            _ = write!(id, "-{}", name);
        }
        SharedString::from(id).into()
    }

    fn item_value_id(&self) -> ElementId {
        use std::fmt::Write;
        let mut id = match &self.entry {
            DapEntry::Watcher(watcher) => format!("watcher-{}", watcher.expression),
            DapEntry::Variable(dap) => format!("variable-{}", dap.name),
            DapEntry::Scope(dap) => format!("scope-{}", dap.name),
        };
        for name in self.path.indices.iter() {
            _ = write!(id, "-{}", name);
        }
        _ = write!(id, "-value");
        SharedString::from(id).into()
    }
}

struct VariableColor {
    name: Option<Hsla>,
    value: Option<Hsla>,
}

pub struct VariableList {
    entries: Vec<ListEntry>,
    max_width_index: Option<usize>,
    entry_states: HashMap<EntryPath, EntryState>,
    selected_stack_frame_id: Option<StackFrameId>,
    list_handle: UniformListScrollHandle,
    session: Entity<Session>,
    selection: Option<EntryPath>,
    open_context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    focus_handle: FocusHandle,
    edited_path: Option<(EntryPath, Entity<Editor>)>,
    disabled: bool,
    memory_view: Entity<MemoryView>,
    weak_running: WeakEntity<RunningState>,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for VariableList {
    fn focus_handle(&self, _: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

use crate::{Keep, KeepAll, OpenAgentDiff, Reject, RejectAll};
use acp_thread::{AcpThread, AcpThreadEvent};
use action_log::{ActionLogTelemetry, LastRejectUndo};
use agent_settings::AgentSettings;
use anyhow::Result;
use buffer_diff::DiffHunkStatus;
use collections::{HashMap, HashSet};
use editor::{
    Direction, Editor, EditorEvent, EditorSettings, MultiBuffer, MultiBufferSnapshot,
    SelectionEffects, SplittableEditor, ToPoint,
    actions::{GoToHunk, GoToPreviousHunk},
    multibuffer_context_lines,
    scroll::Autoscroll,
};

use gpui::{
    Action, AnyElement, App, AppContext, Empty, Entity, EventEmitter, FocusHandle, Focusable,
    Global, SharedString, Subscription, Task, TaskExt, WeakEntity, Window, prelude::*,
};

use language::{Buffer, Capability, OffsetRangeExt, Point};
use mav_actions::assistant::ToggleFocus;
use multi_buffer::PathKey;
use project::{Project, ProjectItem, ProjectPath};
use settings::{Settings, SettingsStore};
use std::{
    any::{Any, TypeId},
    collections::hash_map::Entry,
    ops::Range,
    sync::Arc,
};
use ui::{CommonAnimationExt, IconButtonShape, KeyBinding, Tooltip, prelude::*, vertical_divider};
use util::ResultExt;
use workspace::{
    Item, ItemHandle, ItemNavHistory, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView,
    Workspace,
    item::{ItemEvent, SaveOptions, TabContentParams},
    searchable::SearchableItemHandle,
};

#[path = "agent_diff/editor_addon.rs"]
mod editor_addon;
#[path = "agent_diff/global_registry.rs"]
mod global_registry;
#[path = "agent_diff/global_review.rs"]
mod global_review;
#[path = "agent_diff/hunk_controls.rs"]
mod hunk_controls;
#[path = "agent_diff/pane.rs"]
mod pane;
#[path = "agent_diff/pane_item.rs"]
mod pane_item;
#[path = "agent_diff/review.rs"]
mod review;
#[cfg(test)]
#[path = "agent_diff/tests.rs"]
mod tests;
#[path = "agent_diff/toolbar.rs"]
mod toolbar;

pub use editor_addon::EditorAgentDiffAddon;
use global_registry::WorkspaceThread;
pub use global_registry::{AgentDiff, EditorState};
use hunk_controls::diff_hunk_controls;
pub use pane::AgentDiffPane;
use review::{
    keep_edits_in_ranges, keep_edits_in_selection, reject_edits_in_ranges,
    reject_edits_in_selection,
};
pub use toolbar::{AgentDiffToolbar, AgentDiffToolbarItem};

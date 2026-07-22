use std::{cmp, path::PathBuf, process::ExitStatus, sync::Arc, time::Duration};

use crate::{
    TerminalView, default_working_directory,
    persistence::{
        SerializedItems, SerializedTerminalPanel, deserialize_terminal_panel, serialize_pane_group,
    },
};
use breadcrumbs::Breadcrumbs;
use collections::HashMap;
use db::kvp::KeyValueStore;
use futures::{channel::oneshot, future::join_all};
use gpui::{
    Action, Anchor, AnyView, App, AsyncApp, AsyncWindowContext, Context, Entity, EventEmitter,
    FocusHandle, Focusable, IntoElement, ParentElement, Pixels, Render, Styled, Task, TaskExt,
    WeakEntity, Window, actions,
};
use itertools::Itertools;
use project::{Fs, Project};

use settings::{Settings, TerminalDockPosition};
use task::{RevealStrategy, RevealTarget, Shell, ShellBuilder, SpawnInTerminal, TaskId};
use terminal::{Terminal, terminal_settings::TerminalSettings};
use ui::{
    ButtonLike, Clickable, ContextMenu, FluentBuilder, PopoverMenu, SplitButton, Toggleable,
    Tooltip, prelude::*,
};
use util::{ResultExt, TryFutureExt};
use workspace::{
    ActivateNextPane, ActivatePane, ActivatePaneDown, ActivatePaneLeft, ActivatePaneRight,
    ActivatePaneUp, ActivatePreviousPane, DraggedTab, ItemId, MoveItemToPane,
    MoveItemToPaneInDirection, MovePaneDown, MovePaneLeft, MovePaneRight, MovePaneUp, Pane,
    PaneGroup, SplitDirection, SplitDown, SplitLeft, SplitMode, SplitRight, SplitUp, SwapPaneDown,
    SwapPaneLeft, SwapPaneRight, SwapPaneUp, ToggleZoom, Workspace,
    dock::{DockPosition, Panel, PanelEvent, PanelHandle},
    item::SerializableItem,
    move_active_item, pane,
};

use anyhow::{Result, anyhow};
use mav_actions::assistant::InlineAssist;

const TERMINAL_PANEL_KEY: &str = "TerminalPanel";

actions!(
    terminal_panel,
    [
        /// Toggles the terminal panel.
        Toggle,
        /// Toggles focus on the terminal panel.
        ToggleFocus
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _: &mut Context<Workspace>| {
            workspace.register_action(TerminalPanel::new_terminal);
            workspace.register_action(TerminalPanel::open_terminal);
            workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
                if is_enabled_in_workspace(workspace, cx) {
                    workspace.toggle_panel_focus::<TerminalPanel>(window, cx);
                }
            });
            workspace.register_action(|workspace, _: &Toggle, window, cx| {
                if is_enabled_in_workspace(workspace, cx) {
                    if !workspace.toggle_panel_focus::<TerminalPanel>(window, cx) {
                        workspace.close_panel::<TerminalPanel>(window, cx);
                    }
                }
            });
        },
    )
    .detach();
}

mod core;
mod failed_spawn;
mod load;
mod pane_lifecycle;
mod panel_impl;
mod panel_trait;
mod provider;
mod render;
mod spawn;
mod task_pane;

#[cfg(test)]
mod tests;

pub struct TerminalPanel {
    pub(crate) active_pane: Entity<Pane>,
    pub(crate) center: PaneGroup,
    fs: Arc<dyn Fs>,
    workspace: WeakEntity<Workspace>,
    pending_serialization: Task<Option<()>>,
    pending_terminals_to_add: usize,
    deferred_tasks: HashMap<TaskId, Task<()>>,
    assistant_enabled: bool,
    assistant_tab_bar_button: Option<AnyView>,
    active: bool,
}

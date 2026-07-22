pub(crate) mod breakpoint_list;
mod config;
pub(crate) mod console;
mod constructor;
mod debug_terminal;
mod debugger_pane;
pub(crate) mod loaded_source_list;
pub(crate) mod memory_view;
pub(crate) mod module_list;
mod pane_items;
mod scenario_resolution;
mod session_control;
pub mod stack_frame_list;
mod sub_view;
pub mod variable_list;

pub use debug_terminal::DebugTerminal;
pub(crate) use debugger_pane::new_debugger_pane;
use std::{
    any::Any,
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::Duration,
};
pub(crate) use sub_view::SubView;

use crate::{
    ToggleExpandItem,
    attach_modal::{AttachModal, ModalIntent},
    new_process_modal::resolve_path,
    persistence::{self, DebuggerPaneItem, SerializedLayout},
    session::running::memory_view::MemoryView,
};

use anyhow::{Context as _, Result, anyhow, bail};
use breakpoint_list::BreakpointList;
use collections::{HashMap, IndexMap};
use console::Console;
use dap::{
    Capabilities, DapRegistry, RunInTerminalRequestArguments, Thread,
    adapters::{DebugAdapterName, DebugTaskDefinition},
    client::SessionId,
    debugger_settings::DebuggerSettings,
};
use futures::{SinkExt, channel::mpsc};
use gpui::{
    Action as _, AnyView, AppContext, Axis, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    NoAction, Pixels, Point, Subscription, Task, TaskExt, WeakEntity,
};
use language::Buffer;
use loaded_source_list::LoadedSourceList;
use module_list::ModuleList;
use project::{
    DebugScenarioContext, Project, WorktreeId,
    debugger::session::{self, Session, SessionEvent, SessionStateEvent, ThreadId, ThreadStatus},
};
use rpc::proto::ViewId;
use serde_json::Value;
use settings::Settings;
use stack_frame_list::StackFrameList;
use task::{
    BuildTaskDefinition, DebugScenario, MavDebugConfig, SharedTaskContext, Shell, ShellBuilder,
    SpawnInTerminal, TaskContext, substitute_variables_in_str,
};
use terminal_view::TerminalView;
use ui::{
    FluentBuilder, IntoElement, Render, StatefulInteractiveElement, Tab, Tooltip, VisibleOnHover,
    VisualContext, prelude::*,
};
use util::ResultExt;
use variable_list::VariableList;
use workspace::{
    ActivePaneDecorator, DraggedTab, Item, ItemHandle, Member, Pane, PaneGroup, SplitDirection,
    Workspace, item::TabContentParams, move_item, pane::Event,
};

static PROCESS_ID_PLACEHOLDER: LazyLock<String> =
    LazyLock::new(|| task::VariableName::PickProcessId.template_value());

pub struct RunningState {
    session: Entity<Session>,
    thread_id: Option<ThreadId>,
    focus_handle: FocusHandle,
    _remote_id: Option<ViewId>,
    workspace: WeakEntity<Workspace>,
    project: WeakEntity<Project>,
    session_id: SessionId,
    variable_list: Entity<variable_list::VariableList>,
    _subscriptions: Vec<Subscription>,
    stack_frame_list: Entity<stack_frame_list::StackFrameList>,
    loaded_sources_list: Entity<LoadedSourceList>,
    pub debug_terminal: Entity<DebugTerminal>,
    module_list: Entity<module_list::ModuleList>,
    console: Entity<Console>,
    breakpoint_list: Entity<BreakpointList>,
    panes: PaneGroup,
    active_pane: Entity<Pane>,
    pane_close_subscriptions: HashMap<EntityId, Subscription>,
    dock_axis: Axis,
    _schedule_serialize: Option<Task<()>>,
    pub(crate) scenario: Option<DebugScenario>,
    pub(crate) scenario_context: Option<DebugScenarioContext>,
    memory_view: Entity<MemoryView>,
}

impl RunningState {
    pub(crate) fn thread_id(&self) -> Option<ThreadId> {
        self.thread_id
    }

    pub(crate) fn active_pane(&self) -> &Entity<Pane> {
        &self.active_pane
    }
}

impl Render for RunningState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let zoomed_pane = self
            .panes
            .panes()
            .into_iter()
            .find(|pane| pane.read(cx).is_zoomed());

        let active = self.panes.panes().into_iter().next();
        let pane = if let Some(zoomed_pane) = zoomed_pane {
            zoomed_pane.update(cx, |pane, cx| pane.render(window, cx).into_any_element())
        } else if let Some(active) = active {
            self.panes
                .render(
                    None,
                    &ActivePaneDecorator::new(active, &self.workspace),
                    window,
                    cx,
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let thread_status = self
            .thread_id
            .map(|thread_id| self.session.read(cx).thread_status(thread_id))
            .unwrap_or(ThreadStatus::Exited);

        self.variable_list.update(cx, |this, cx| {
            this.disabled(thread_status != ThreadStatus::Stopped, cx);
        });
        v_flex()
            .size_full()
            .key_context("DebugSessionItem")
            .track_focus(&self.focus_handle(cx))
            .child(h_flex().flex_1().child(pane))
    }
}

impl Focusable for RunningState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        debugger_panel::DebugPanel,
        tests::{init_test, init_test_workspace, start_debug_session},
    };
    use gpui::{BackgroundExecutor, TestAppContext, VisualTestContext};
    use project::{FakeFs, Project};
    use serde_json::json;
    use util::path;

    #[gpui::test]
    async fn stale_subview_host_during_tab_drop_does_not_read_updating_source_pane(
        executor: BackgroundExecutor,
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(executor);
        fs.insert_tree(
            path!("/project"),
            json!({
                "main.rs": "fn main() {}",
            }),
        )
        .await;

        let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
        let workspace = init_test_workspace(&project, cx).await;
        let cx = &mut VisualTestContext::from_window(*workspace, cx);

        start_debug_session(&workspace, cx, |_| {}).expect("debug session starts");
        cx.run_until_parked();

        let running_state = workspace
            .update(cx, |multi_workspace, _window, cx| {
                multi_workspace.workspace().update(cx, |workspace, cx| {
                    let debug_panel = workspace.panel::<DebugPanel>(cx).expect("debug panel");
                    let active_session = debug_panel
                        .read(cx)
                        .active_session()
                        .expect("active debug session");
                    active_session.read(cx).running_state().clone()
                })
            })
            .expect("workspace update succeeds");

        let (source_pane, stale_host_pane) = running_state.read_with(cx, |running_state, _| {
            let panes = running_state.panes.panes();
            let mut panes = panes.into_iter();
            let source_pane = panes.next().expect("source pane").clone();
            let stale_host_pane = panes.next().expect("stale host pane").clone();
            (source_pane, stale_host_pane)
        });

        let dragged_tab = {
            let source_pane_entity = source_pane.clone();
            source_pane.read_with(cx, |source_pane, _| {
                let item = source_pane
                    .item_for_index(0)
                    .expect("source pane contains debugger subview")
                    .boxed_clone();
                DraggedTab {
                    pane: source_pane_entity,
                    item,
                    ix: 0,
                    detail: 0,
                    is_active: true,
                }
            })
        };

        let active_subview = source_pane.read_with(cx, |source_pane, _| {
            source_pane
                .active_item()
                .and_then(|item| item.downcast::<SubView>())
                .expect("active item is a debugger subview")
        });
        active_subview.update(cx, |subview, _| {
            subview.set_host_pane(stale_host_pane.downgrade());
        });

        source_pane.update_in(cx, |source_pane, window, cx| {
            source_pane.handle_tab_drop(
                &dragged_tab,
                source_pane.active_item_index(),
                true,
                window,
                cx,
            );
        });
    }
}

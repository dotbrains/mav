use std::process::ExitStatus;

use anyhow::Result;
use gpui::{App, Context, Entity, Task};
use language::Buffer;
use project::{WorktreeId, debugger::session::ThreadStatus};
use task::{DebugScenario, SharedTaskContext, SpawnInTerminal};
use ui::Window;

use crate::{Spawn, Workspace};

pub trait TerminalProvider {
    fn spawn(
        &self,
        task: SpawnInTerminal,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Option<Result<ExitStatus>>>;
}

pub trait DebuggerProvider {
    // `active_buffer` is used to resolve build task's name against language-specific tasks.
    fn start_session(
        &self,
        definition: DebugScenario,
        task_context: SharedTaskContext,
        active_buffer: Option<Entity<Buffer>>,
        worktree_id: Option<WorktreeId>,
        window: &mut Window,
        cx: &mut App,
    );

    fn spawn_task_or_modal(
        &self,
        workspace: &mut Workspace,
        action: &Spawn,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    );

    fn task_scheduled(&self, cx: &mut App);
    fn debug_scenario_scheduled(&self, cx: &mut App);
    fn debug_scenario_scheduled_last(&self, cx: &App) -> bool;

    fn active_thread_state(&self, cx: &App) -> Option<ThreadStatus>;
}

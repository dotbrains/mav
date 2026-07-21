use agent_client_protocol::schema::v1 as acp;
use anyhow::Result;
use futures::FutureExt as _;
use gpui::{App, AsyncApp, Entity, SharedString, Task};
use project::Project;
use settings::Settings;
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use crate::SandboxFallbackDecision;
use crate::sandboxing::{NetworkRequest, sandboxing_enabled_for_project};
use crate::{AgentTool, ThreadEnvironment, ToolCallEventStream, ToolInput};
#[cfg(test)]
use input::TerminalSandboxInput;
use input::TerminalToolRequest;
pub use input::{SandboxedTerminalToolInput, TerminalToolInput};
#[cfg(test)]
use output::TerminalOutputSelection;
use output::process_content;
#[cfg(test)]
use output::select_terminal_output_lines;
use runner::run_terminal_tool;
use sandbox_git_paths::{SandboxGitPathCandidates, sandbox_git_paths};
#[cfg(test)]
use sandbox_request::join_write_paths;
use sandbox_request::{
    build_network_request, network_request_to_sandbox_network_access, resolve_write_paths,
};

#[path = "terminal_tool/input.rs"]
mod input;
#[path = "terminal_tool/output.rs"]
mod output;
#[path = "terminal_tool/runner.rs"]
mod runner;
pub(crate) mod sandbox_git_paths;
#[path = "terminal_tool/sandbox_request.rs"]
mod sandbox_request;

const COMMAND_OUTPUT_LIMIT: u64 = 16 * 1024;

pub struct TerminalTool {
    project: Entity<Project>,
    environment: Rc<dyn ThreadEnvironment>,
}

impl TerminalTool {
    pub fn new(project: Entity<Project>, environment: Rc<dyn ThreadEnvironment>) -> Self {
        Self {
            project,
            environment,
        }
    }
}

pub struct SandboxedTerminalTool {
    project: Entity<Project>,
    environment: Rc<dyn ThreadEnvironment>,
}

impl SandboxedTerminalTool {
    pub fn new(project: Entity<Project>, environment: Rc<dyn ThreadEnvironment>) -> Self {
        Self {
            project,
            environment,
        }
    }
}

impl AgentTool for TerminalTool {
    type Input = TerminalToolInput;
    type Output = String;

    const NAME: &'static str = "terminal";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Execute
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        terminal_initial_title(input.map(|input| input.command))
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |cx| {
            let input = input.recv().await.map_err(|e| e.to_string())?;
            run_terminal_tool(
                self.project.clone(),
                self.environment.clone(),
                input.into(),
                event_stream,
                cx,
            )
            .await
        })
    }
}

impl AgentTool for SandboxedTerminalTool {
    type Input = SandboxedTerminalToolInput;
    type Output = String;

    const NAME: &'static str = "sandboxed_terminal";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Execute
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        terminal_initial_title(input.map(|input| input.command))
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |cx| {
            let input = input.recv().await.map_err(|e| e.to_string())?;
            run_terminal_tool(
                self.project.clone(),
                self.environment.clone(),
                input.into(),
                event_stream,
                cx,
            )
            .await
        })
    }
}

fn terminal_initial_title(input: Result<String, serde_json::Value>) -> SharedString {
    if let Ok(command) = input {
        command.into()
    } else {
        "".into()
    }
}

/// Resolve model-requested write paths into absolute paths.
///
/// Relative paths are resolved against the command's working directory when
/// known, otherwise against the project's first worktree root. Paths that
/// can't be made absolute (relative paths with no base) are dropped. The
/// resulting paths are shown to the user for approval, so resolving against
/// model-controlled inputs is safe — nothing is granted without that prompt.

fn working_dir(cd: &str, project: &Entity<Project>, cx: &mut App) -> Result<Option<PathBuf>> {
    let project = project.read(cx);

    if cd == "." || cd.is_empty() {
        let mut worktrees = project.worktrees(cx);

        match worktrees.next() {
            Some(worktree) => {
                anyhow::ensure!(
                    worktrees.next().is_none(),
                    "'.' is ambiguous in multi-root workspaces. Please specify a root directory explicitly.",
                );
                Ok(Some(worktree.read(cx).abs_path().to_path_buf()))
            }
            None => Ok(None),
        }
    } else {
        let input_path = Path::new(cd);

        if input_path.is_absolute() {
            if project
                .worktrees(cx)
                .any(|worktree| input_path.starts_with(&worktree.read(cx).abs_path()))
            {
                return Ok(Some(input_path.into()));
            }
        } else if let Some(worktree) = project.worktree_for_root_name(cd, cx) {
            return Ok(Some(worktree.read(cx).abs_path().to_path_buf()));
        }

        anyhow::bail!("`cd` directory {cd:?} was not in any of the project's worktrees.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod initial_title_tests {
        use super::*;

        include!("terminal_tool_tests/initial_title.rs");
    }

    mod output_tests {
        use super::*;

        include!("terminal_tool_tests/output.rs");
    }

    mod run_tests {
        use super::*;

        include!("terminal_tool_tests/run.rs");
    }

    mod validation_tests {
        use super::*;

        include!("terminal_tool_tests/validation.rs");
    }

    mod env_prefix_tests {
        use super::*;

        include!("terminal_tool_tests/env_prefix.rs");
    }

    mod write_path_tests {
        use super::*;

        include!("terminal_tool_tests/write_paths.rs");
    }

    mod sandbox_permission_tests {
        use super::*;

        include!("terminal_tool_tests/sandbox_permissions.rs");
    }

    mod sandbox_floor_tests {
        use super::*;

        include!("terminal_tool_tests/sandbox_floor.rs");
    }
}

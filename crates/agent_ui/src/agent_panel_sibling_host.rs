use agent_client_protocol::schema::v1 as acp;
use anyhow::{Context as _, Result, anyhow};
use gpui::{App, AsyncApp, SharedString, Task, WeakEntity};
use language_model::LanguageModelRegistry;

use crate::{
    Agent, AgentInitialContent, AgentPanel, AgentThreadSource, agent_panel::CreateThreadOptions,
};

/// Bridges agent-side `SiblingThreadHost` calls to `AgentPanel`. Constructed
/// and installed on a `NativeAgent` by the agent panel when a native-agent
/// thread is created.
pub(crate) struct AgentPanelSiblingHost {
    panel: WeakEntity<AgentPanel>,
    window: gpui::AnyWindowHandle,
}

impl AgentPanelSiblingHost {
    pub(crate) fn new(panel: WeakEntity<AgentPanel>, window: gpui::AnyWindowHandle) -> Self {
        Self { panel, window }
    }
}

impl agent::SiblingThreadHost for AgentPanelSiblingHost {
    fn create_sibling_thread(
        &self,
        request: agent::SiblingThreadRequest,
        cx: &mut AsyncApp,
    ) -> Task<Result<agent::SiblingThreadInfo>> {
        let panel = self.panel.clone();
        let window = self.window;
        cx.spawn(async move |cx| {
            let agent_choice = match request.agent_id.as_deref() {
                None => None,
                Some(id) if id == agent::MAV_AGENT_ID.as_ref() => Some(Agent::NativeAgent),
                Some(id) => {
                    let known = panel
                        .read_with(cx, |panel, cx| {
                            let store = panel.project.read(cx).agent_server_store().clone();
                            store
                                .read(cx)
                                .external_agents()
                                .any(|known_id| known_id.0.as_ref() == id)
                        })
                        .unwrap_or(false);
                    if !known {
                        return Err(anyhow!(
                            "Unknown agent id {id:?}. Call `list_agents_and_models` \
                             to see the agents available for `create_thread`."
                        ));
                    }
                    Some(Agent::Custom {
                        id: project::AgentId(id.to_string().into()),
                    })
                }
            };

            let initial_content = AgentInitialContent::ContentBlock {
                blocks: vec![acp::ContentBlock::Text(acp::TextContent::new(
                    request.prompt.clone(),
                ))],
                auto_submit: true,
            };

            let title: SharedString = request.title.clone();
            let options = CreateThreadOptions {
                title: Some(title.clone()),
                initial_content: Some(initial_content),
                agent: agent_choice.clone(),
                model: request.model.clone(),
                work_dirs: None,
            };

            let mut worktree_warning = None;
            let target_panel = if request.use_new_worktree {
                let workspace = panel.read_with(cx, |panel, _cx| panel.workspace.clone())?;
                let workspace = workspace
                    .upgrade()
                    .ok_or_else(|| anyhow!("Source workspace is no longer available"))?;
                let branch_target = match request.base_ref.as_ref() {
                    Some(ref_name) => mav_actions::NewWorktreeBranchTarget::ExistingBranch {
                        name: ref_name.clone(),
                    },
                    None => mav_actions::NewWorktreeBranchTarget::CurrentBranch,
                };
                let action = mav_actions::CreateWorktree {
                    worktree_name: request.worktree_name.clone(),
                    branch_target,
                };
                let creation = window.update(cx, |_root, window, cx| {
                    workspace.update(cx, |workspace, cx| {
                        git_ui::worktree_service::create_worktree_workspace(
                            workspace, &action, window, None, cx,
                        )
                    })
                })?;
                let created = creation
                    .await
                    .context("failed to create worktree workspace")?;
                if created.consolidated_worktrees {
                    worktree_warning = Some(
                        "The project contained multiple worktrees backed by the same git \
                         repository, so they were consolidated into a single new worktree. \
                         The new thread's worktree is based on one of them and may not \
                         reflect the exact state of the others."
                            .to_string(),
                    );
                }
                created
                    .workspace
                    .read_with(cx, |workspace, cx| workspace.panel::<AgentPanel>(cx))
                    .ok_or_else(|| anyhow!("new workspace did not register an agent panel"))?
                    .downgrade()
            } else {
                panel.clone()
            };

            let resolved_agent_id = window.update(cx, |_root, window, cx| {
                target_panel.update(cx, |panel, cx| {
                    panel.create_thread_with_options(
                        options,
                        AgentThreadSource::AgentPanel,
                        window,
                        cx,
                    );
                    let resolved_agent = agent_choice
                        .clone()
                        .unwrap_or_else(|| panel.selected_agent.clone());
                    resolved_agent.id()
                })
            })??;

            Ok(agent::SiblingThreadInfo {
                title,
                agent_id: resolved_agent_id.0.to_string(),
                model: request.model,
                warning: worktree_warning,
            })
        })
    }

    fn list_available_agents(&self, cx: &mut App) -> Result<agent::AvailableAgents> {
        let panel = self
            .panel
            .upgrade()
            .ok_or_else(|| anyhow!("Agent panel is no longer available"))?;

        let mut agents = Vec::new();
        let native_models = {
            let registry = LanguageModelRegistry::read_global(cx);
            let default = registry.default_model();
            let mut models = Vec::new();
            for provider in registry.providers() {
                if !provider.is_authenticated(cx) {
                    continue;
                }
                let provider_id = provider.id();
                for model in provider.provided_models(cx) {
                    let id = format!("{}/{}", provider_id.0, model.id().0);
                    let is_default = default
                        .as_ref()
                        .map(|cm| cm.provider.id() == provider_id && cm.model.id() == model.id())
                        .unwrap_or(false);
                    models.push(agent::AvailableModel {
                        id,
                        name: model.name().0,
                        is_default,
                    });
                }
            }
            models
        };
        agents.push(agent::AvailableAgent {
            id: agent::MAV_AGENT_ID.to_string(),
            name: Agent::NativeAgent.label(),
            is_native: true,
            models: native_models,
        });

        let project = panel.read(cx).project.clone();
        let agent_server_store = project.read(cx).agent_server_store().clone();
        let store = agent_server_store.read(cx);
        for agent_id in store.external_agents() {
            let display = store
                .agent_display_name(agent_id)
                .unwrap_or_else(|| agent_id.0.clone());
            agents.push(agent::AvailableAgent {
                id: agent_id.0.to_string(),
                name: display,
                is_native: false,
                models: Vec::new(),
            });
        }

        Ok(agent::AvailableAgents { agents })
    }
}

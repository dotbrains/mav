pub(crate) const FEATURED_AGENT_IDS: &[&str] =
    &["claude-acp", "codex-acp", "github-copilot-cli", "cursor"];

fn render_registry_agent_button(
    agent: &RegistryAgent,
    installed: bool,
    cx: &mut App,
) -> impl IntoElement {
    let agent_id = agent.id().to_string();
    let element_id = format!("{}-onboarding", agent_id);

    let icon = match agent.icon_path() {
        Some(icon_path) => Icon::from_external_svg(icon_path.clone()),
        None => Icon::new(IconName::Sparkle),
    }
    .size(IconSize::XSmall)
    .color(Color::Muted);

    let fs = <dyn Fs>::global(cx);

    let state_element = if installed {
        Icon::new(IconName::Check)
            .size(IconSize::Small)
            .color(Color::Success)
            .into_any_element()
    } else {
        Label::new("Install")
            .size(LabelSize::XSmall)
            .color(Color::Muted)
            .into_any_element()
    };

    AgentSetupButton::new(element_id)
        .icon(icon)
        .name(agent.name().clone())
        .state(state_element)
        .disabled(installed)
        .on_click(move |_, _, cx| {
            telemetry::event!("Welcome Agent Install Clicked", agent = agent_id.as_str());
            let agent_id = agent_id.clone();
            update_settings_file(fs.clone(), cx, move |settings, _| {
                let agent_servers = settings.agent_servers.get_or_insert_default();
                agent_servers.entry(agent_id).or_insert_with(|| {
                    CustomAgentServerSettings::Registry {
                        env: Default::default(),
                        default_mode: None,
                        default_config_options: HashMap::default(),
                        favorite_config_option_values: HashMap::default(),
                    }
                });
            });
        })
}

fn render_mav_agent_button(user_store: &Entity<UserStore>, cx: &mut App) -> impl IntoElement {
    let client = Client::global(cx);
    let status = *client.status().borrow();

    let plan = user_store.read(cx).plan();
    let is_free = matches!(plan, Some(Plan::MavFree) | None);
    let is_pro = matches!(plan, Some(Plan::MavPro));
    let is_trial = matches!(plan, Some(Plan::MavProTrial));

    let is_signed_out = status.is_signed_out()
        || matches!(
            status,
            client::Status::AuthenticationError | client::Status::ConnectionError
        );
    let is_signing_in = status.is_signing_in();
    let is_signed_in = !is_signed_out;

    let state_element = if is_signed_out {
        Label::new("Sign In")
            .size(LabelSize::XSmall)
            .color(Color::Muted)
            .into_any_element()
    } else if is_signing_in {
        Label::new("Signing In…")
            .size(LabelSize::XSmall)
            .color(Color::Muted)
            .with_animation(
                "signing-in",
                Animation::new(Duration::from_secs(2))
                    .repeat()
                    .with_easing(pulsating_between(0.4, 0.8)),
                |label, delta| label.alpha(delta),
            )
            .into_any_element()
    } else if is_signed_in && is_free {
        Label::new("Start Free Trial")
            .size(LabelSize::XSmall)
            .color(Color::Muted)
            .into_any_element()
    } else {
        Icon::new(IconName::Check)
            .size(IconSize::Small)
            .color(Color::Success)
            .into_any_element()
    };

    AgentSetupButton::new("mav-agent-onboarding")
        .icon(
            Icon::new(IconName::MavAgent)
                .size(IconSize::XSmall)
                .color(Color::Muted),
        )
        .name("Mav Agent")
        .state(state_element)
        .disabled(is_trial || is_pro)
        .map(|this| {
            if is_signed_in && is_free {
                this.on_click(move |_, _window, cx| {
                    telemetry::event!("Start Trial Clicked", state = "post-sign-in");
                    cx.open_url(&mav_urls::start_trial_url(cx))
                })
            } else {
                this.on_click(move |_, _, cx| {
                    telemetry::event!("Welcome Mav Agent Sign In Clicked");
                    let client = Client::global(cx);
                    cx.spawn(async move |cx| client.sign_in_with_optional_connect(true, cx).await)
                        .detach_and_log_err(cx);
                })
            }
        })
}

pub(super) fn render_ai_section(user_store: &Entity<UserStore>, cx: &mut App) -> impl IntoElement {
    let registry_agents = AgentRegistryStore::try_global(cx)
        .map(|store| store.read(cx).agents().to_vec())
        .unwrap_or_default();

    let installed_agents = cx
        .global::<SettingsStore>()
        .get::<AllAgentServersSettings>(None)
        .clone();

    let column_count = 1 + FEATURED_AGENT_IDS.len() as u16;

    let grid = FEATURED_AGENT_IDS.iter().fold(
        div()
            .w_full()
            .mt_1p5()
            .grid()
            .grid_cols(column_count)
            .gap_2()
            .child(render_mav_agent_button(user_store, cx)),
        |grid, agent_id| {
            let Some(agent) = registry_agents
                .iter()
                .find(|a| a.id().as_ref() == *agent_id)
            else {
                return grid;
            };
            let is_installed = installed_agents.contains_key(*agent_id);
            grid.child(render_registry_agent_button(agent, is_installed, cx))
        },
    );

    v_flex()
        .gap_0p5()
        .child(Label::new("Agent Setup"))
        .child(
            Label::new("Install your favorite agents and start your first thread.")
                .color(Color::Muted),
        )
        .child(grid)
}
use std::time::Duration;

use client::{Client, UserStore, mav_urls};
use cloud_api_types::Plan;
use collections::HashMap;
use fs::Fs;
use gpui::{Animation, AnimationExt, App, Entity, IntoElement, TaskExt, pulsating_between};
use project::{AgentRegistryStore, RegistryAgent, agent_server_store::AllAgentServersSettings};
use settings::{CustomAgentServerSettings, SettingsStore, update_settings_file};
use ui::{AgentSetupButton, prelude::*};

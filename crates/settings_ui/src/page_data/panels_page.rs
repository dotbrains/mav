use super::panels_git::git_panel_section;
use super::panels_outline::{
    agent_panel_section, collaboration_panel_section, outline_panel_section,
};
use super::panels_project::project_panel_section;
use super::*;

pub(super) fn panels_page() -> SettingsPage {
    SettingsPage {
        title: "Panels",
        items: concat_sections![
            project_panel_section(),
            outline_panel_section(),
            git_panel_section(),
            collaboration_panel_section(),
            agent_panel_section(),
        ],
    }
}

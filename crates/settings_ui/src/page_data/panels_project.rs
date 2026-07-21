use super::panels_project_behavior::project_panel_behavior_section;
use super::panels_project_display::project_panel_display_section;
use super::*;

pub(super) fn project_panel_section() -> Box<[SettingsPageItem]> {
    concat_sections!(
        project_panel_display_section(),
        project_panel_behavior_section(),
    )
}

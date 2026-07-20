use crate::project_settings::{LspSettings, ProjectSettings};
use gpui::App;
use language::LspAdapterDelegate;
use lsp::LanguageServerName;
use settings::Settings;
use settings::SettingsLocation;
use util::rel_path::RelPath;

pub fn language_server_settings<'a>(
    delegate: &'a dyn LspAdapterDelegate,
    language: &LanguageServerName,
    cx: &'a App,
) -> Option<&'a LspSettings> {
    language_server_settings_for(
        SettingsLocation {
            worktree_id: delegate.worktree_id(),
            path: RelPath::empty(),
        },
        language,
        cx,
    )
}

pub fn language_server_settings_for<'a>(
    location: SettingsLocation<'a>,
    language: &LanguageServerName,
    cx: &'a App,
) -> Option<&'a LspSettings> {
    ProjectSettings::get(Some(location), cx).lsp.get(language)
}

use std::cmp::Ordering::{Equal, Greater, Less};
use std::fmt;
use std::sync::Arc;

use anyhow::Context as _;
use fs::Fs;
use futures::channel::oneshot;
use gpui::App;
use settings::{
    DockPosition, DockSide, SettingsContent, SettingsStore, update_settings_file,
    update_settings_file_with_completion,
};

use crate::AgentSettings;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelLayout {
    pub(crate) agent_dock: Option<DockPosition>,
    pub(crate) project_panel_dock: Option<DockSide>,
    pub(crate) outline_panel_dock: Option<DockSide>,
    pub(crate) collaboration_panel_dock: Option<DockPosition>,
    pub(crate) git_panel_dock: Option<DockPosition>,
}

impl PanelLayout {
    const AGENT: Self = Self {
        agent_dock: Some(DockPosition::Left),
        project_panel_dock: Some(DockSide::Right),
        outline_panel_dock: Some(DockSide::Right),
        collaboration_panel_dock: Some(DockPosition::Right),
        git_panel_dock: Some(DockPosition::Right),
    };

    const EDITOR: Self = Self {
        agent_dock: Some(DockPosition::Right),
        project_panel_dock: Some(DockSide::Left),
        outline_panel_dock: Some(DockSide::Left),
        collaboration_panel_dock: Some(DockPosition::Left),
        git_panel_dock: Some(DockPosition::Left),
    };

    pub fn is_agent_layout(&self) -> bool {
        *self == Self::AGENT
    }

    pub fn is_editor_layout(&self) -> bool {
        *self == Self::EDITOR
    }

    fn read_from(content: &SettingsContent) -> Self {
        Self {
            agent_dock: content.agent.as_ref().and_then(|a| a.dock),
            project_panel_dock: content.project_panel.as_ref().and_then(|p| p.dock),
            outline_panel_dock: content.outline_panel.as_ref().and_then(|p| p.dock),
            collaboration_panel_dock: content.collaboration_panel.as_ref().and_then(|p| p.dock),
            git_panel_dock: content.git_panel.as_ref().and_then(|p| p.dock),
        }
    }

    fn write_to(&self, settings: &mut SettingsContent) {
        settings.agent.get_or_insert_default().dock = self.agent_dock;
        settings.project_panel.get_or_insert_default().dock = self.project_panel_dock;
        settings.outline_panel.get_or_insert_default().dock = self.outline_panel_dock;
        settings.collaboration_panel.get_or_insert_default().dock = self.collaboration_panel_dock;
        settings.git_panel.get_or_insert_default().dock = self.git_panel_dock;
    }

    fn write_diff_to(&self, current_merged: &PanelLayout, settings: &mut SettingsContent) {
        if self.agent_dock != current_merged.agent_dock {
            settings.agent.get_or_insert_default().dock = self.agent_dock;
        }
        if self.project_panel_dock != current_merged.project_panel_dock {
            settings.project_panel.get_or_insert_default().dock = self.project_panel_dock;
        }
        if self.outline_panel_dock != current_merged.outline_panel_dock {
            settings.outline_panel.get_or_insert_default().dock = self.outline_panel_dock;
        }
        if self.collaboration_panel_dock != current_merged.collaboration_panel_dock {
            settings.collaboration_panel.get_or_insert_default().dock =
                self.collaboration_panel_dock;
        }
        if self.git_panel_dock != current_merged.git_panel_dock {
            settings.git_panel.get_or_insert_default().dock = self.git_panel_dock;
        }
    }

    fn backfill_to(&self, user_layout: &PanelLayout, settings: &mut SettingsContent) {
        if user_layout.agent_dock.is_none() {
            settings.agent.get_or_insert_default().dock = self.agent_dock;
        }
        if user_layout.project_panel_dock.is_none() {
            settings.project_panel.get_or_insert_default().dock = self.project_panel_dock;
        }
        if user_layout.outline_panel_dock.is_none() {
            settings.outline_panel.get_or_insert_default().dock = self.outline_panel_dock;
        }
        if user_layout.collaboration_panel_dock.is_none() {
            settings.collaboration_panel.get_or_insert_default().dock =
                self.collaboration_panel_dock;
        }
        if user_layout.git_panel_dock.is_none() {
            settings.git_panel.get_or_insert_default().dock = self.git_panel_dock;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowLayout {
    Editor(Option<PanelLayout>),
    Agent(Option<PanelLayout>),
    Custom(PanelLayout),
}

impl WindowLayout {
    pub fn agent() -> Self {
        Self::Agent(None)
    }

    pub fn editor() -> Self {
        Self::Editor(None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutoCompactThreshold {
    /// Compact once the context window is at least this full, as a fraction in
    /// the range `(0.0, 1.0]`.
    Percentage(f64),
    /// Compact once at least this many tokens have been used.
    TokensUsed(u64),
    /// Compact once fewer than this many tokens remain in the context window.
    TokensRemaining(u64),
}

impl AutoCompactThreshold {
    /// The threshold used when none is configured, or when the configured value
    /// is invalid (90% of the context window).
    pub const DEFAULT: Self = Self::Percentage(0.9);
}

impl fmt::Display for AutoCompactThreshold {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Percentage(percent) => write!(formatter, "{}%", percent * 100.0),
            Self::TokensUsed(tokens) => write!(formatter, "{tokens}"),
            Self::TokensRemaining(tokens) => write!(formatter, "-{tokens}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoCompactSettings {
    pub enabled: bool,
    pub threshold: AutoCompactThreshold,
}

pub(crate) fn parse_auto_compact_threshold(raw: &str) -> anyhow::Result<AutoCompactThreshold> {
    let trimmed = raw.trim();
    if let Some(percent) = trimmed.strip_suffix('%') {
        let value: f64 = percent
            .trim_end()
            .parse()
            .with_context(|| format!("invalid auto_compact threshold percentage {raw:?}"))?;
        anyhow::ensure!(
            value > 0.0 && value <= 100.0,
            "auto_compact threshold percentage must be between 0% and 100%, got {raw:?}"
        );
        Ok(AutoCompactThreshold::Percentage(value / 100.0))
    } else {
        let tokens: i64 = trimmed.parse().with_context(|| {
            format!(
                "invalid auto_compact threshold {raw:?}; \
                 expected a percentage like \"90%\" or an integer number of tokens"
            )
        })?;
        match tokens.cmp(&0) {
            Greater => Ok(AutoCompactThreshold::TokensUsed(tokens as u64)),
            Less => Ok(AutoCompactThreshold::TokensRemaining(tokens.unsigned_abs())),
            Equal => {
                anyhow::bail!("auto_compact threshold of 0 is not valid")
            }
        }
    }
}

impl AgentSettings {
    pub fn get_layout(cx: &App) -> WindowLayout {
        let store = cx.global::<SettingsStore>();
        let merged = store.merged_settings();
        let user_layout = store
            .raw_user_settings()
            .map(|u| PanelLayout::read_from(u.content.as_ref()))
            .unwrap_or_default();
        let merged_layout = PanelLayout::read_from(merged);

        if merged_layout.is_agent_layout() {
            return WindowLayout::Agent(Some(user_layout));
        }

        if merged_layout.is_editor_layout() {
            return WindowLayout::Editor(Some(user_layout));
        }

        WindowLayout::Custom(user_layout)
    }

    pub fn backfill_editor_layout(fs: Arc<dyn Fs>, cx: &App) {
        let user_layout = cx
            .global::<SettingsStore>()
            .raw_user_settings()
            .map(|u| PanelLayout::read_from(u.content.as_ref()))
            .unwrap_or_default();

        update_settings_file(fs, cx, move |settings, _cx| {
            PanelLayout::EDITOR.backfill_to(&user_layout, settings);
        });
    }

    pub fn set_layout(
        layout: WindowLayout,
        fs: Arc<dyn Fs>,
        cx: &App,
    ) -> oneshot::Receiver<anyhow::Result<()>> {
        let merged = PanelLayout::read_from(cx.global::<SettingsStore>().merged_settings());

        match layout {
            WindowLayout::Agent(None) => {
                update_settings_file_with_completion(fs, cx, move |settings, _cx| {
                    PanelLayout::AGENT.write_diff_to(&merged, settings);
                })
            }
            WindowLayout::Editor(None) => {
                update_settings_file_with_completion(fs, cx, move |settings, _cx| {
                    PanelLayout::EDITOR.write_diff_to(&merged, settings);
                })
            }
            WindowLayout::Agent(Some(saved))
            | WindowLayout::Editor(Some(saved))
            | WindowLayout::Custom(saved) => {
                update_settings_file_with_completion(fs, cx, move |settings, _cx| {
                    saved.write_to(settings);
                })
            }
        }
    }
}

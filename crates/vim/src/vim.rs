//! Vim support for Mav.

#[cfg(test)]
mod test;

mod change_list;
mod command;
mod digraph;
mod helix;
mod indent;
mod insert;
mod mode_indicator;
mod motion;
mod normal;
mod object;
mod replace;
mod rewrap;
mod state;
mod surrounds;
mod visual;

use crate::normal::paste::Paste as VimPaste;
use collections::HashMap;
use editor::{
    Anchor, Bias, Editor, EditorEvent, EditorSettings, MultiBufferOffset, NavigationOverlayKey,
    NavigationTargetOverlay, SelectionEffects,
    actions::Paste,
    display_map::ToDisplayPoint,
    movement::{self, FindRange},
};
use gpui::{
    Action, App, AppContext, Axis, Context, Entity, EventEmitter, Focusable, KeyContext,
    KeystrokeEvent, Render, Subscription, Task, WeakEntity, Window, actions,
};
use insert::{NormalBefore, TemporaryNormal};
use language::{CursorShape, Point, Selection, SelectionGoal, TransactionId};
pub use mode_indicator::ModeIndicator;
use motion::Motion;
use multi_buffer::ToPoint as _;
use normal::search::SearchSubmit;
use object::Object;
use schemars::JsonSchema;
use search::BufferSearchBar;
use serde::Deserialize;
use settings::RegisterSetting;
pub use settings::{
    ModeContent, Settings, SettingsStore, UseSystemClipboard, update_settings_file,
};
use state::{
    HelixJumpBehaviour, HelixJumpLabel, Mode, Operator, RecordedSelection, SearchState, VimGlobals,
};
use std::{mem, ops::Range, sync::Arc};
use surrounds::SurroundsType;
use theme_settings::ThemeSettings;
use ui::{IntoElement, SharedString, px};
use vim_mode_setting::HelixModeSetting;
use vim_mode_setting::VimModeSetting;
use workspace::{self, Pane, Workspace};

use crate::{
    normal::{GoToPreviousTab, GoToTab},
    state::ReplayableAction,
};

mod actions_def;
mod activation;
mod core;
mod focus_record;
mod helix_jump;
mod init;
mod input_ignored;
mod mode;
mod settings;
mod transactions;

impl editor::Addon for VimAddon {
    fn extend_key_context(&self, key_context: &mut KeyContext, cx: &App) {
        self.entity.read(cx).extend_key_context(key_context, cx)
    }

    fn to_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The state pertaining to Vim mode.
pub(crate) struct Vim {
    pub(crate) mode: Mode,
    pub last_mode: Mode,
    pub temp_mode: bool,
    pub status_label: Option<SharedString>,
    pub exit_temporary_mode: bool,

    operator_stack: Vec<Operator>,
    pub(crate) replacements: Vec<(Range<editor::Anchor>, String)>,

    pub(crate) stored_visual_mode: Option<(Mode, Vec<bool>)>,

    pub(crate) current_tx: Option<TransactionId>,
    pub(crate) current_anchor: Option<Selection<Anchor>>,
    pub(crate) undo_modes: HashMap<TransactionId, Mode>,
    pub(crate) undo_last_line_tx: Option<TransactionId>,
    extended_pending_selection_id: Option<usize>,

    selected_register: Option<char>,
    pub search: SearchState,

    editor: WeakEntity<Editor>,

    last_command: Option<String>,
    running_command: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

// Hack: Vim intercepts events dispatched to a window and updates the view in response.
// This means it needs a VisualContext. The easiest way to satisfy that constraint is
// to make Vim a "View" that is just never actually rendered.
impl Render for Vim {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

enum VimEvent {
    Focused,
}
impl EventEmitter<VimEvent> for Vim {}

impl Vim {
    /// The namespace for Vim actions.
    const NAMESPACE: &'static str = "vim";
}

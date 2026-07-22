use crate::command::command_interceptor;
use crate::motion::MotionKind;
use crate::normal::repeat::Replayer;
use crate::surrounds::SurroundsType;
use crate::{ToggleMarksView, ToggleRegistersView, UseSystemClipboard, Vim, VimAddon, VimSettings};
use crate::{motion::Motion, object::Object};
use anyhow::Result;
use collections::HashMap;
use command_palette_hooks::{CommandPaletteFilter, GlobalCommandPaletteInterceptor};
use db::{
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use editor::display_map::{is_invisible, replacement};
use editor::{Anchor, ClipboardSelection, Editor, MultiBuffer, ToPoint as EditorToPoint};
use gpui::{
    Action, App, AppContext, BorrowAppContext, ClipboardEntry, ClipboardItem, DismissEvent, Entity,
    EntityId, Global, HighlightStyle, StyledText, Subscription, Task, TaskExt, TextStyle,
    WeakEntity,
};
use language::{Buffer, BufferEvent, BufferId, Chunk, LanguageAwareStyling, Point};

use multi_buffer::MultiBufferRow;
use picker::{Picker, PickerDelegate};
use project::{Project, ProjectItem, ProjectPath};
use serde::{Deserialize, Serialize};
use settings::{Settings, SettingsStore};
use std::borrow::BorrowMut;
use std::collections::HashSet;
use std::path::Path;
use std::{fmt::Display, ops::Range, sync::Arc};
use text::{Bias, ToPoint};
use theme_settings::ThemeSettings;
use ui::{
    ActiveTheme, Context, Div, FluentBuilder, KeyBinding, ParentElement, SharedString, Styled,
    StyledTypography, Window, h_flex, rems,
};
use util::ResultExt;
use util::rel_path::RelPath;
use workspace::searchable::Direction;
use workspace::{MultiWorkspace, Workspace, WorkspaceDb, WorkspaceId};

#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
    VisualBlock,
    HelixNormal,
    HelixSelect,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Normal => write!(f, "NORMAL"),
            Mode::Insert => write!(f, "INSERT"),
            Mode::Replace => write!(f, "REPLACE"),
            Mode::Visual => write!(f, "VISUAL"),
            Mode::VisualLine => write!(f, "VISUAL LINE"),
            Mode::VisualBlock => write!(f, "VISUAL BLOCK"),
            Mode::HelixNormal => write!(f, "NORMAL"),
            Mode::HelixSelect => write!(f, "SELECT"),
        }
    }
}

impl Mode {
    pub fn is_visual(&self) -> bool {
        match self {
            Self::Visual | Self::VisualLine | Self::VisualBlock | Self::HelixSelect => true,
            Self::Normal | Self::Insert | Self::Replace | Self::HelixNormal => false,
        }
    }

    pub fn is_helix(&self) -> bool {
        matches!(self, Self::HelixNormal | Self::HelixSelect)
    }

    /// `HelixNormal` qualifies because its cursor is itself a one-character selection.
    pub fn has_selection(&self) -> bool {
        self.is_visual() || matches!(self, Self::HelixNormal)
    }

    pub fn is_normal(&self) -> bool {
        matches!(self, Self::Normal | Self::HelixNormal)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
    Change,
    Delete,
    Yank,
    Replace,
    Object {
        around: bool,
    },
    FindForward {
        before: bool,
        multiline: bool,
    },
    FindBackward {
        after: bool,
        multiline: bool,
    },
    Sneak {
        first_char: Option<char>,
    },
    SneakBackward {
        first_char: Option<char>,
    },
    AddSurrounds {
        // Typically no need to configure this as `SendKeystrokes` can be used - see #23088.
        target: Option<SurroundsType>,
    },
    ChangeSurrounds {
        target: Option<Object>,
        /// Represents whether the opening bracket was used for the target
        /// object.
        opening: bool,
        /// Computed anchors for the opening and closing bracket characters,
        bracket_anchors: Vec<Option<(Anchor, Anchor)>>,
    },
    DeleteSurrounds,
    Mark,
    Jump {
        line: bool,
    },
    Indent,
    Outdent,
    AutoIndent,
    Rewrap,
    ShellCommand,
    Lowercase,
    Uppercase,
    OppositeCase,
    Rot13,
    Rot47,
    Digraph {
        first_char: Option<char>,
    },
    Literal {
        prefix: Option<String>,
    },
    Register,
    RecordRegister,
    ReplayRegister,
    ToggleComments,
    ToggleBlockComments,
    ReplaceWithRegister,
    Exchange,
    HelixMatch,
    HelixNext {
        around: bool,
    },
    HelixPrevious {
        around: bool,
    },
    HelixSurroundAdd,
    HelixSurroundReplace {
        replaced_char: Option<char>,
    },
    HelixSurroundDelete,
    HelixJump {
        behaviour: HelixJumpBehaviour,
        first_char: Option<char>,
        labels: Vec<HelixJumpLabel>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct HelixJumpLabel {
    pub label: [char; 2],
    pub range: Range<Anchor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelixJumpBehaviour {
    Move,
    MoveToWordStart,
    Extend,
    ExtendToWordStart,
}

#[derive(Default, Clone, Debug)]
pub enum RecordedSelection {
    #[default]
    None,
    Visual {
        rows: u32,
        cols: u32,
    },
    SingleLine {
        cols: u32,
    },
    VisualBlock {
        rows: u32,
        cols: u32,
    },
    VisualLine {
        rows: u32,
    },
}

#[derive(Default, Clone, Debug)]
pub struct Register {
    pub(crate) text: SharedString,
    pub(crate) clipboard_selections: Option<Vec<ClipboardSelection>>,
}

impl From<Register> for ClipboardItem {
    fn from(register: Register) -> Self {
        if let Some(clipboard_selections) = register.clipboard_selections {
            ClipboardItem::new_string_with_json_metadata(register.text.into(), clipboard_selections)
        } else {
            ClipboardItem::new_string(register.text.into())
        }
    }
}

impl From<ClipboardItem> for Register {
    fn from(item: ClipboardItem) -> Self {
        match item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::String(value) => Some(value),
            _ => None,
        }) {
            Some(value) => Register {
                text: value.text().to_owned().into(),
                clipboard_selections: value.metadata_json::<Vec<ClipboardSelection>>(),
            },
            None => Register::default(),
        }
    }
}

impl From<String> for Register {
    fn from(text: String) -> Self {
        Register {
            text: text.into(),
            clipboard_selections: None,
        }
    }
}

#[derive(Default)]
pub struct VimGlobals {
    pub last_find: Option<Motion>,

    pub dot_recording: bool,
    pub dot_replaying: bool,

    /// pre_count is the number before an operator is specified (3 in 3d2d)
    pub pre_count: Option<usize>,
    /// post_count is the number after an operator is specified (2 in 3d2d)
    pub post_count: Option<usize>,
    pub forced_motion: bool,
    pub stop_recording_after_next_action: bool,
    pub ignore_current_insertion: bool,
    pub recording_count: Option<usize>,
    pub recorded_count: Option<usize>,
    pub recording_actions: Vec<ReplayableAction>,
    pub recorded_actions: Vec<ReplayableAction>,
    pub recorded_selection: RecordedSelection,

    /// The register being written to by the active `q{register}` macro
    /// recording.
    pub recording_register: Option<char>,
    /// The register that was selected at the start of the current
    /// dot-recording, for example, `"ap`.
    pub recording_register_for_dot: Option<char>,
    /// The register from the last completed dot-recording. Used when replaying
    /// with `.`.
    pub recorded_register_for_dot: Option<char>,
    pub last_recorded_register: Option<char>,
    pub last_replayed_register: Option<char>,
    pub replayer: Option<Replayer>,

    pub last_yank: Option<SharedString>,
    pub registers: HashMap<char, Register>,
    pub recordings: HashMap<char, Vec<ReplayableAction>>,

    pub focused_vim: Option<WeakEntity<Vim>>,

    pub marks: HashMap<EntityId, Entity<MarksState>>,
}

pub struct MarksState {
    workspace: WeakEntity<Workspace>,

    multibuffer_marks: HashMap<EntityId, HashMap<String, Vec<Anchor>>>,
    buffer_marks: HashMap<BufferId, HashMap<String, Vec<text::Anchor>>>,
    watched_buffers: HashMap<BufferId, (MarkLocation, Subscription, Subscription)>,

    serialized_marks: HashMap<Arc<Path>, HashMap<String, Vec<Point>>>,
    global_marks: HashMap<String, MarkLocation>,

    _subscription: Subscription,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum MarkLocation {
    Buffer(EntityId),
    Path(Arc<Path>),
}

pub enum Mark {
    Local(Vec<Anchor>),
    Buffer(EntityId, Vec<Anchor>),
    Path(Arc<Path>, Vec<Point>),
}

mod db;
mod globals;
mod marks_state;
mod marks_view;
mod operator;
mod registers_view;

impl Vim {
    pub fn globals(cx: &mut App) -> &mut VimGlobals {
        cx.global_mut::<VimGlobals>()
    }

    pub fn update_globals<C, R>(cx: &mut C, f: impl FnOnce(&mut VimGlobals, &mut C) -> R) -> R
    where
        C: BorrowMut<App>,
    {
        cx.update_global(f)
    }
}

#[derive(Debug)]
pub enum ReplayableAction {
    Action(Box<dyn Action>),
    Insertion {
        text: Arc<str>,
        utf16_range_to_replace: Option<Range<isize>>,
    },
}

impl Clone for ReplayableAction {
    fn clone(&self) -> Self {
        match self {
            Self::Action(action) => Self::Action(action.boxed_clone()),
            Self::Insertion {
                text,
                utf16_range_to_replace,
            } => Self::Insertion {
                text: text.clone(),
                utf16_range_to_replace: utf16_range_to_replace.clone(),
            },
        }
    }
}

#[derive(Default, Debug)]
pub struct SearchState {
    pub direction: Direction,
    pub count: usize,
    pub cmd_f_search: bool,

    pub prior_selections: Vec<Range<Anchor>>,
    pub prior_operator: Option<Operator>,
    pub prior_mode: Mode,
    pub helix_select: bool,
    pub _dismiss_subscription: Option<gpui::Subscription>,
}

use anyhow::{Result, anyhow};
use collections::{HashMap, HashSet};
use command_palette_hooks::{CommandInterceptItem, CommandInterceptResult};
use editor::{
    Bias, Editor, EditorSettings, SelectionEffects, ToPoint,
    actions::{SortLinesCaseInsensitive, SortLinesCaseSensitive},
    display_map::ToDisplayPoint,
};
use futures::AsyncWriteExt as _;
use gpui::{
    Action, App, AppContext as _, Context, Global, Keystroke, Task, TaskExt, WeakEntity, Window,
    actions,
};
use itertools::Itertools;
use language::Point;
use mav_actions::{OpenDocs, RevealTarget};
use multi_buffer::MultiBufferRow;
use project::ProjectPath;
use regex::Regex;
use schemars::JsonSchema;
use search::{BufferSearchBar, SearchOptions};
use serde::Deserialize;
use settings::{Settings, SettingsStore};
use std::{
    iter::Peekable,
    ops::{Deref, Range},
    path::{Path, PathBuf},
    process::Stdio,
    str::Chars,
    sync::OnceLock,
    time::Instant,
};
use task::{HideStrategy, RevealStrategy, SaveStrategy, Shell, SpawnInTerminal, TaskId};
use ui::ActiveTheme;
use util::{
    ResultExt,
    paths::PathStyle,
    rel_path::{RelPath, RelPathBuf},
};
use workspace::{Item, SaveIntent, Workspace, notifications::NotifyResultExt};
use workspace::{SplitDirection, notifications::DetachAndPromptErr};

#[path = "command/options.rs"]
mod options;

use options::{VimOption, VimSet};

use crate::{
    ToggleMarksView, ToggleRegistersView, Vim, VimSettings,
    motion::{EndOfDocument, Motion, MotionKind, StartOfDocument},
    normal::{
        JoinLines,
        search::{FindCommand, ReplaceCommand, Replacement},
    },
    object::Object,
    rewrap::Rewrap,
    state::{Mark, Mode},
    visual::VisualDeleteLine,
};

mod catalog;
mod interceptor;
mod matching_lines;
mod parser;
mod register;
mod register_file_commands;
mod register_normal_commands;
mod register_range_commands;
mod register_save;
mod register_setup;
mod shell;
mod types;

use catalog::{act_on_range, commands, select_range, wrap_count};
pub use interceptor::command_interceptor;
use matching_lines::OnMatchingLines;
use parser::{CommandRange, Position, VimCommand};
pub use register::register;
pub use shell::ShellExec;
use types::*;

#[cfg(test)]
#[path = "command/tests.rs"]
mod test;

mod neovim_backed_test_context;
mod neovim_connection;
mod vim_test_context;

use std::{sync::Arc, time::Duration};

use collections::HashMap;
use command_palette::CommandPalette;
use editor::{
    AnchorRangeExt, DisplayPoint, Editor, EditorMode, MultiBuffer, MultiBufferOffset,
    actions::{DeleteLine, WrapSelectionsInTag},
    code_context_menus::CodeContextMenu,
    display_map::DisplayRow,
    test::editor_test_context::EditorTestContext,
};
use futures::StreamExt;
use gpui::{KeyBinding, Modifiers, MouseButton, TestAppContext, px};
use itertools::Itertools;
use language::{CursorShape, Language, LanguageConfig, Point};
pub use neovim_backed_test_context::*;
use settings::{CommandAliasTarget, SettingsStore};
use ui::Pixels;
use util::{path, test::marked_text_ranges};
pub use vim_test_context::*;

use gpui::VisualTestContext;
use indoc::indoc;
use project::FakeFs;
use search::BufferSearchBar;
use search::{ProjectSearchView, project_search};
use serde_json::json;
use workspace::{DeploySearch, MultiWorkspace};

use crate::{PushSneak, PushSneakBackward, VimAddon, insert::NormalBefore, motion, state::Mode};

use util_macros::perf;

mod basic_modes;
mod comments_sneak;
mod completion_lsp;
mod folded_paragraphs;
mod jk_mappings;
mod misc_ui;
mod mouse_marks;
mod remap_record;
mod search_motions;
mod wrapped_folds;

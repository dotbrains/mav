use std::{fmt::Write, time::Duration};

use editor::{HighlightKey, MultiBufferOffset};
use gpui::{KeyBinding, UpdateGlobal, VisualTestContext};
use indoc::indoc;
use language::{CursorShape, Point};
use project::FakeFs;
use search::{ProjectSearchView, project_search};
use serde_json::json;
use settings::{SettingsStore, ThemeColorsContent, ThemeStyleContent};
use theme::ActiveTheme as _;
use util::path;
use workspace::{DeploySearch, MultiWorkspace};

use super::{HELIX_JUMP_LABEL_LIMIT, HelixJumpToWord};
use crate::{
    HELIX_JUMP_OVERLAY_KEY, Vim, VimAddon,
    state::{Mode, Operator},
    test::VimTestContext,
};

#[path = "test_delete.rs"]
mod test_delete;

#[path = "test_delete_character_end_of_line.rs"]
mod test_delete_character_end_of_line;

#[path = "test_f_and_t.rs"]
mod test_f_and_t;

#[path = "test_newline_char.rs"]
mod test_newline_char;

#[path = "test_insert_selected.rs"]
mod test_insert_selected;

#[path = "test_append.rs"]
mod test_append;

#[path = "test_replace.rs"]
mod test_replace;

#[path = "test_replace_with_crlf.rs"]
mod test_replace_with_crlf;

#[path = "test_helix_yank.rs"]
mod test_helix_yank;

#[path = "test_shift_r_paste.rs"]
mod test_shift_r_paste;

#[path = "test_helix_select_mode.rs"]
mod test_helix_select_mode;

#[path = "test_insert_mode_stickiness.rs"]
mod test_insert_mode_stickiness;

#[path = "test_helix_select_append.rs"]
mod test_helix_select_append;

#[path = "test_goto_last_modification.rs"]
mod test_goto_last_modification;

#[path = "test_helix_select_lines.rs"]
mod test_helix_select_lines;

#[path = "test_helix_insert_before_after_select_lines.rs"]
mod test_helix_insert_before_after_select_lines;

#[path = "test_helix_insert_before_after_helix_select.rs"]
mod test_helix_insert_before_after_helix_select;

#[path = "test_helix_select_mode_motion.rs"]
mod test_helix_select_mode_motion;

#[path = "test_helix_select_end_of_line.rs"]
mod test_helix_select_end_of_line;

#[path = "test_helix_select_mode_motion_multiple_cursors.rs"]
mod test_helix_select_mode_motion_multiple_cursors;

#[path = "test_helix_select_word_motions.rs"]
mod test_helix_select_word_motions;

#[path = "test_exit_visual_mode.rs"]
mod test_exit_visual_mode;

#[path = "test_helix_select_motion.rs"]
mod test_helix_select_motion;

#[path = "test_helix_full_cursor_selection.rs"]
mod test_helix_full_cursor_selection;

#[path = "test_helix_motion_on_unrendered_editor.rs"]
mod test_helix_motion_on_unrendered_editor;

#[path = "test_helix_select_regex.rs"]
mod test_helix_select_regex;

#[path = "test_helix_select_next_match.rs"]
mod test_helix_select_next_match;

#[path = "test_helix_select_next_match_wrapping.rs"]
mod test_helix_select_next_match_wrapping;

#[path = "test_helix_select_next_match_wrapping_from_normal.rs"]
mod test_helix_select_next_match_wrapping_from_normal;

#[path = "test_helix_select_star_then_match.rs"]
mod test_helix_select_star_then_match;

#[path = "test_helix_substitute.rs"]
mod test_helix_substitute;

#[path = "test_g_l_end_of_line.rs"]
mod test_g_l_end_of_line;

#[path = "test_project_search_opens_in_normal_mode.rs"]
mod test_project_search_opens_in_normal_mode;

#[path = "test_scroll_with_selection.rs"]
mod test_scroll_with_selection;

#[path = "test_helix_insert_end_of_line.rs"]
mod test_helix_insert_end_of_line;

#[path = "test_helix_replace_uses_graphemes.rs"]
mod test_helix_replace_uses_graphemes;

#[path = "test_helix_start_of_document.rs"]
mod test_helix_start_of_document;

#[path = "test_helix_end_of_document.rs"]
mod test_helix_end_of_document;

#[path = "test_helix_go_to_hunk.rs"]
mod test_helix_go_to_hunk;

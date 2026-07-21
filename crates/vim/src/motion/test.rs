use crate::{
    motion::{
        Matching, NextSubwordEnd, NextSubwordStart, PreviousSubwordEnd, PreviousSubwordStart,
    },
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};
use editor::{
    Editor, EditorMode, Inlay, MultiBuffer, test::editor_test_context::EditorTestContext,
};
use gpui::KeyBinding;
use indoc::indoc;
use language::Point;
use multi_buffer::MultiBufferRow;

mod line_window;
mod matching_core;
mod matching_language;
mod percentage_indent_forced;
mod subword_diff;
mod word_inlay;

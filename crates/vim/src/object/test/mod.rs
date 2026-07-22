use editor::{Editor, EditorMode, MultiBuffer, test::editor_test_context::EditorTestContext};
use gpui::KeyBinding;
use indoc::indoc;
use text::Point;

use crate::{
    object::{AnyBrackets, AnyQuotes, MiniBrackets},
    state::Mode,
    test::{NeovimBackedTestContext, VimTestContext},
};

mod arguments_indent;
mod arrows;
mod brackets;
mod paragraph;
mod quotes;
mod subword;
mod surrounding;
mod tags;
mod word;

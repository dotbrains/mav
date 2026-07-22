use crate::{SystemColors, ThemeColors};

mod cool;
mod dark;
mod greens;
mod light;
mod monochrome;
mod neutral;
mod purples;
mod reds;
mod scale_types;
mod scales;
mod vcs_colors;
mod warm;

pub(crate) use cool::{cyan, jade, sky, teal};
pub(crate) use greens::{grass, green, lime, mint};
pub(crate) use monochrome::{black, white};
pub(crate) use neutral::{gray, mauve, olive, sage, sand, slate};
pub(crate) use purples::{blue, indigo, iris, plum, purple, violet};
pub(crate) use reds::{crimson, pink, red, ruby};
pub use scales::default_color_scales;
pub(crate) use warm::{amber, bronze, brown, gold, orange, tomato, yellow};

use scale_types::StaticColorScaleSet;
use vcs_colors::{
    ADDED_COLOR, MODIFIED_COLOR, REMOVED_COLOR, WORD_ADDED_COLOR, WORD_DELETED_COLOR,
};

pub(crate) fn neutral() -> crate::scale::ColorScaleSet {
    sand()
}

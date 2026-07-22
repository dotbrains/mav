use super::*;
use crate::scale::ColorScales;

/// Color scales used to build the default themes.
pub fn default_color_scales() -> ColorScales {
    ColorScales {
        gray: gray(),
        mauve: mauve(),
        slate: slate(),
        sage: sage(),
        olive: olive(),
        sand: sand(),
        gold: gold(),
        bronze: bronze(),
        brown: brown(),
        yellow: yellow(),
        amber: amber(),
        orange: orange(),
        tomato: tomato(),
        red: red(),
        ruby: ruby(),
        crimson: crimson(),
        pink: pink(),
        plum: plum(),
        purple: purple(),
        violet: violet(),
        iris: iris(),
        indigo: indigo(),
        blue: blue(),
        cyan: cyan(),
        teal: teal(),
        jade: jade(),
        green: green(),
        grass: grass(),
        lime: lime(),
        mint: mint(),
        sky: sky(),
        black: black(),
        white: white(),
    }
}

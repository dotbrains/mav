use crate::ColorScale;
use crate::scale::ColorScaleSet;
use gpui::{Hsla, Rgba};

pub(super) type StaticColorScale = [&'static str; 12];

pub(super) struct StaticColorScaleSet {
    pub(super) scale: &'static str,
    pub(super) light: StaticColorScale,
    pub(super) light_alpha: StaticColorScale,
    pub(super) dark: StaticColorScale,
    pub(super) dark_alpha: StaticColorScale,
}

impl TryFrom<StaticColorScaleSet> for ColorScaleSet {
    type Error = anyhow::Error;

    fn try_from(value: StaticColorScaleSet) -> Result<Self, Self::Error> {
        fn to_color_scale(scale: StaticColorScale) -> anyhow::Result<ColorScale> {
            scale
                .into_iter()
                .map(|color| Rgba::try_from(color).map(Hsla::from))
                .collect::<Result<Vec<_>, _>>()
                .map(ColorScale::from_iter)
        }

        Ok(Self::new(
            value.scale,
            to_color_scale(value.light)?,
            to_color_scale(value.light_alpha)?,
            to_color_scale(value.dark)?,
            to_color_scale(value.dark_alpha)?,
        ))
    }
}

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationTargetOverlay {
    pub target_range: Range<Anchor>,
    pub label: NavigationOverlayLabel,
    pub covered_text_range: Option<Range<Anchor>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationOverlayLabel {
    pub text: SharedString,
    pub text_color: Hsla,
    pub x_offset: Pixels,
    pub scale_factor: f32,
}

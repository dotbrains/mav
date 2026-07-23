use super::*;
use crate::{
    MultiBuffer,
    display_map::{HighlightKey, InlayHighlights},
    hover_links::InlayHighlight,
};
use collections::HashMap;
use gpui::{App, HighlightStyle};
use multi_buffer::Anchor;
use project::{InlayHint, InlayHintLabel, ResolveState};
use rand::prelude::*;
use settings::SettingsStore;
use std::{cmp::Reverse, env, sync::Arc};
use sum_tree::TreeMap;
use text::{BufferId, Patch, Rope};
use util::RandomCharIter;
use util::post_inc;

mod basic;
mod padding;
mod random;
mod utf8;

pub(crate) fn init_test(cx: &mut App) {
    let store = SettingsStore::test(cx);
    cx.set_global(store);
    theme_settings::init(theme::LoadThemes::JustBase, cx);
}

/// Helper to create test highlights for an inlay
pub(crate) fn create_inlay_highlights(
    inlay_id: InlayId,
    highlight_range: Range<usize>,
    position: Anchor,
) -> TreeMap<HighlightKey, TreeMap<InlayId, (HighlightStyle, InlayHighlight)>> {
    let mut inlay_highlights = TreeMap::default();
    let mut type_highlights = TreeMap::default();
    type_highlights.insert(
        inlay_id,
        (
            HighlightStyle::default(),
            InlayHighlight {
                inlay: inlay_id,
                range: highlight_range,
                inlay_position: position,
            },
        ),
    );
    inlay_highlights.insert(HighlightKey::Editor, type_highlights);
    inlay_highlights
}

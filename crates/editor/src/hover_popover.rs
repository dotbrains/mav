use crate::{
    Anchor, AnchorRangeExt, DisplayPoint, DisplayRow, Editor, EditorSettings, EditorSnapshot,
    GlobalDiagnosticRenderer, HighlightKey, Hover,
    display_map::{InlayOffset, ToDisplayPoint, is_invisible},
    editor_settings::EditorSettingsScrollbarProxy,
    hover_links::{InlayHighlight, RangeInEditor},
    movement::TextLayoutDetails,
    scroll::ScrollAmount,
};
use anyhow::Context as _;
use gpui::{
    AnyElement, App, AsyncWindowContext, Bounds, Context, Entity, Focusable as _, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, ScrollHandle, Size,
    StatefulInteractiveElement, StyleRefinement, Styled, Subscription, Task, TaskExt,
    TextStyleRefinement, Window, canvas, div, px,
};
use itertools::Itertools;
use language::{DiagnosticEntry, Language, LanguageRegistry};
use lsp::DiagnosticSeverity;
use markdown::{CopyButtonVisibility, Markdown, MarkdownElement, MarkdownStyle};
use multi_buffer::{MultiBufferOffset, ToOffset, ToPoint};
use project::{HoverBlock, HoverBlockKind, InlayHintLabelPart};
use settings::Settings;
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
};
use std::{ops::Range, sync::Arc, time::Duration};
use std::{path::PathBuf, rc::Rc};
use theme_settings::ThemeSettings;
use ui::{CopyButton, Scrollbars, WithScrollbar, prelude::*, theme_is_transparent};
use url::Url;
use util::TryFutureExt;
use workspace::{OpenOptions, OpenVisible, Workspace};

pub const MIN_POPOVER_CHARACTER_WIDTH: f32 = 20.;
pub const MIN_POPOVER_LINE_HEIGHT: f32 = 4.;
pub const POPOVER_RIGHT_OFFSET: Pixels = px(8.0);
pub const HOVER_POPOVER_GAP: Pixels = px(10.);

/// Bindable action which uses the most recent selection head to trigger a hover
pub fn hover(editor: &mut Editor, _: &Hover, window: &mut Window, cx: &mut Context<Editor>) {
    let head = editor.selections.newest_anchor().head();
    show_hover(editor, head, true, window, cx);
}

/// The internal hover action dispatches between `show_hover` or `hide_hover`
/// depending on whether a point to hover over is provided.
pub fn hover_at(
    editor: &mut Editor,
    anchor: Option<Anchor>,
    mouse_position: Option<gpui::Point<Pixels>>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    if EditorSettings::get_global(cx).hover_popover_enabled {
        if show_keyboard_hover(editor, window, cx) {
            return;
        }

        if let Some(anchor) = anchor {
            editor.hover_state.hiding_delay_task = None;
            editor.hover_state.closest_mouse_distance = None;
            show_hover(editor, anchor, false, window, cx);
        } else if !editor.hover_state.visible() {
            editor.hover_state.info_task = None;
        } else {
            let settings = EditorSettings::get_global(cx);
            if !settings.hover_popover_sticky {
                hide_hover(editor, cx);
                return;
            }

            let mut getting_closer = false;
            if let Some(mouse_position) = mouse_position {
                getting_closer = editor.hover_state.is_mouse_getting_closer(mouse_position);
            }

            // If we are moving away and a timer is already running, just let it count down.
            if !getting_closer && editor.hover_state.hiding_delay_task.is_some() {
                return;
            }

            // If we are moving closer, or if no timer is running at all, start/restart the timer.
            let delay = Duration::from_millis(settings.hover_popover_hiding_delay.0);
            let task = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                this.update(cx, |editor, cx| {
                    hide_hover(editor, cx);
                })
                .ok();
            });
            editor.hover_state.hiding_delay_task = Some(task);
        }
    }
}

pub fn show_keyboard_hover(
    editor: &mut Editor,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> bool {
    if let Some(anchor) = editor.hover_state.info_popovers.iter().find_map(|p| {
        if *p.keyboard_grace.borrow() {
            p.anchor
        } else {
            None
        }
    }) {
        show_hover(editor, anchor, false, window, cx);
        return true;
    }

    if let Some(anchor) = editor
        .hover_state
        .diagnostic_popover
        .as_ref()
        .and_then(|d| {
            if *d.keyboard_grace.borrow() {
                Some(d.anchor)
            } else {
                None
            }
        })
    {
        show_hover(editor, anchor, false, window, cx);
        return true;
    }

    false
}

pub struct InlayHover {
    pub(crate) range: InlayHighlight,
    pub tooltip: HoverBlock,
}

pub fn find_hovered_hint_part(
    label_parts: Vec<InlayHintLabelPart>,
    hint_start: InlayOffset,
    hovered_offset: InlayOffset,
) -> Option<(InlayHintLabelPart, Range<InlayOffset>)> {
    if hovered_offset >= hint_start {
        let mut offset_in_hint = hovered_offset - hint_start;
        let mut part_start = hint_start;
        for part in label_parts {
            let part_len = part.value.len();
            if offset_in_hint >= part_len {
                offset_in_hint -= part_len;
                part_start.0 += part_len;
            } else {
                let part_end = InlayOffset(part_start.0 + part_len);
                return Some((part, part_start..part_end));
            }
        }
    }
    None
}

pub fn hover_at_inlay(
    editor: &mut Editor,
    inlay_hover: InlayHover,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    if EditorSettings::get_global(cx).hover_popover_enabled {
        if editor.pending_rename.is_some() {
            return;
        }

        let Some(project) = editor.project.clone() else {
            return;
        };

        if editor
            .hover_state
            .info_popovers
            .iter()
            .any(|InfoPopover { symbol_range, .. }| {
                if let RangeInEditor::Inlay(range) = symbol_range
                    && range == &inlay_hover.range
                {
                    // Hover triggered from same location as last time. Don't show again.
                    return true;
                }
                false
            })
        {
            return;
        }

        let hover_popover_delay = EditorSettings::get_global(cx).hover_popover_delay.0;

        editor.hover_state.hiding_delay_task = None;
        editor.hover_state.closest_mouse_distance = None;

        let task = cx.spawn_in(window, async move |this, cx| {
            async move {
                cx.background_executor()
                    .timer(Duration::from_millis(hover_popover_delay))
                    .await;
                this.update(cx, |this, _| {
                    this.hover_state.diagnostic_popover = None;
                })?;

                let language_registry = project.read_with(cx, |p, _| p.languages().clone());
                let blocks = vec![inlay_hover.tooltip];
                let parsed_content = parse_blocks(&blocks, Some(&language_registry), None, cx);

                let scroll_handle = ScrollHandle::new();

                let subscription = this
                    .update(cx, |_, cx| {
                        parsed_content.as_ref().map(|parsed_content| {
                            cx.observe(parsed_content, |_, _, cx| cx.notify())
                        })
                    })
                    .ok()
                    .flatten();

                let hover_popover = InfoPopover {
                    symbol_range: RangeInEditor::Inlay(inlay_hover.range.clone()),
                    parsed_content,
                    scroll_handle,
                    keyboard_grace: Rc::new(RefCell::new(false)),
                    anchor: None,
                    last_bounds: Rc::new(Cell::new(None)),
                    _subscription: subscription,
                };

                this.update(cx, |this, cx| {
                    // TODO: no background highlights happen for inlays currently
                    this.hover_state.info_popovers = vec![hover_popover];
                    cx.notify();
                })?;

                anyhow::Ok(())
            }
            .log_err()
            .await
        });

        editor.hover_state.info_task = Some(task);
    }
}

/// Hides the type information popup.
/// Triggered by the `Hover` action when the cursor is not over a symbol or when the
/// selections changed.
pub fn hide_hover(editor: &mut Editor, cx: &mut Context<Editor>) -> bool {
    let info_popovers = editor.hover_state.info_popovers.drain(..);
    let diagnostics_popover = editor.hover_state.diagnostic_popover.take();
    let did_hide = info_popovers.count() > 0 || diagnostics_popover.is_some();

    editor.hover_state.info_task = None;
    editor.hover_state.hiding_delay_task = None;
    editor.hover_state.closest_mouse_distance = None;

    editor.clear_background_highlights(HighlightKey::HoverState, cx);

    if did_hide {
        cx.notify();
    }

    did_hide
}

/// Queries the LSP and shows type info and documentation
/// about the symbol the mouse is currently hovering over.
/// Triggered by the `Hover` action when the cursor may be over a symbol.
mod markdown;
mod popovers;
mod query;
mod state;
#[cfg(test)]
mod tests;

pub use markdown::{diagnostics_markdown_style, hover_markdown_style};
use markdown::{open_markdown_url, parse_blocks};
pub use popovers::{DiagnosticPopover, InfoPopover};
use query::show_hover;
pub use state::HoverState;

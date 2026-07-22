use crate::{
    Anchor, Editor, EditorSettings, EditorSnapshot, FindAllReferences, GoToDefinition,
    GoToDefinitionSplit, GoToTypeDefinition, GoToTypeDefinitionSplit, GotoDefinitionKind,
    HighlightKey, Navigated, PointForPosition, SelectPhase,
    editor_settings::GoToDefinitionFallback, scroll::ScrollAmount,
};
use gpui::{
    App, AsyncWindowContext, Context, Entity, HighlightStyle, Modifiers, Pixels, Task,
    UnderlineStyle, Window, px,
};
use language::{Bias, ToOffset};
use linkify::{LinkFinder, LinkKind};
use lsp::LanguageServerId;
use project::{InlayId, LocationLink};
use settings::Settings;
use std::{ops::Range, str::FromStr as _};
use text::OffsetRangeExt;
mod file_target;
mod navigation;

pub use file_target::ResolvedFileTarget;
pub(crate) use file_target::find_file;
pub use navigation::show_link_definition;

use theme::ActiveTheme as _;
use util::{ResultExt, TryFutureExt as _};

#[derive(Debug)]
pub struct HoveredLinkState {
    pub last_trigger_point: TriggerPoint,
    pub preferred_kind: GotoDefinitionKind,
    pub symbol_range: Option<RangeInEditor>,
    pub links: Vec<HoverLink>,
    pub task: Option<Task<Option<()>>>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RangeInEditor {
    Text(Range<Anchor>),
    Inlay(InlayHighlight),
}

impl RangeInEditor {
    pub fn as_text_range(&self) -> Option<Range<Anchor>> {
        match self {
            Self::Text(range) => Some(range.clone()),
            Self::Inlay(_) => None,
        }
    }

    pub fn point_within_range(
        &self,
        trigger_point: &TriggerPoint,
        snapshot: &EditorSnapshot,
    ) -> bool {
        match (self, trigger_point) {
            (Self::Text(range), TriggerPoint::Text(point)) => {
                let point_after_start = range.start.cmp(point, &snapshot.buffer_snapshot()).is_le();
                point_after_start && range.end.cmp(point, &snapshot.buffer_snapshot()).is_ge()
            }
            (Self::Inlay(highlight), TriggerPoint::InlayHint(point, _, _)) => {
                highlight.inlay == point.inlay
                    && highlight.range.contains(&point.range.start)
                    && highlight.range.contains(&point.range.end)
            }
            (Self::Inlay(_), TriggerPoint::Text(_))
            | (Self::Text(_), TriggerPoint::InlayHint(_, _, _)) => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum HoverLink {
    Url(String),
    File(ResolvedFileTarget),
    Text(LocationLink),
    /// Navigate to an LSP-given location whose buffer may not be loaded yet.
    /// Used by inlay-hint hover, code-lens references, and document-link
    /// targets that point inside a workspace file (e.g. `file:///foo#9,16`).
    LspLocation(lsp::Location, LanguageServerId),
}

/// Convert a `documentLink` target URI into a [`HoverLink`], reusing the
/// existing navigation paths: `file://` URIs go through the LSP location
/// pipeline (so an optional `#line[,column]` fragment is honored), while
/// any other scheme is opened as a regular URL.
pub fn document_link_target_to_hover_link(target: &str, server_id: LanguageServerId) -> HoverLink {
    if let Ok(url) = url::Url::parse(target)
        && url.scheme() == "file"
        && let Ok(uri) = lsp::Uri::from_str(target)
    {
        let position = url
            .fragment()
            .and_then(parse_uri_fragment_position)
            .unwrap_or_default();
        return HoverLink::LspLocation(
            lsp::Location {
                uri,
                range: lsp::Range::new(position, position),
            },
            server_id,
        );
    }
    HoverLink::Url(target.to_string())
}

/// Parse a URI fragment such as `9,16`, `9:16`, `L9`, or `L9:16` into an
/// LSP position (1-based input, 0-based output). Servers like the JSON
/// language server attach this fragment to `file://` document link
/// targets to point at a specific row/column inside the file.
fn parse_uri_fragment_position(fragment: &str) -> Option<lsp::Position> {
    let stripped = fragment.strip_prefix('L').unwrap_or(fragment);
    let (line_str, column_str) = match stripped.split_once([',', ':']) {
        Some((line, column)) => (line, Some(column)),
        None => (stripped, None),
    };
    let line = line_str.parse::<u32>().ok()?.checked_sub(1)?;
    let character = column_str
        .and_then(|column| column.parse::<u32>().ok())
        .and_then(|column| column.checked_sub(1))
        .unwrap_or(0);
    Some(lsp::Position { line, character })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHighlight {
    pub inlay: InlayId,
    pub inlay_position: Anchor,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TriggerPoint {
    Text(Anchor),
    InlayHint(InlayHighlight, lsp::Location, LanguageServerId),
}

impl TriggerPoint {
    fn anchor(&self) -> &Anchor {
        match self {
            TriggerPoint::Text(anchor) => anchor,
            TriggerPoint::InlayHint(inlay_range, _, _) => &inlay_range.inlay_position,
        }
    }
}

pub fn exclude_link_to_position(
    buffer: &Entity<language::Buffer>,
    current_position: &text::Anchor,
    location: &LocationLink,
    cx: &App,
) -> bool {
    // Exclude definition links that points back to cursor position.
    // (i.e., currently cursor upon definition).
    let snapshot = buffer.read(cx).snapshot();
    !(buffer == &location.target.buffer
        && current_position
            .bias_right(&snapshot)
            .cmp(&location.target.range.start, &snapshot)
            .is_ge()
        && current_position
            .cmp(&location.target.range.end, &snapshot)
            .is_le())
}

impl Editor {
    pub(crate) fn update_hovered_link(
        &mut self,
        point_for_position: PointForPosition,
        mouse_position: Option<gpui::Point<Pixels>>,
        snapshot: &EditorSnapshot,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hovered_link_modifier = Editor::is_cmd_or_ctrl_pressed(&modifiers, cx);
        if !hovered_link_modifier || self.has_pending_selection() {
            self.hide_hovered_link(cx);
            return;
        }

        if !cx.is_cursor_visible() {
            self.hide_hovered_link(cx);
            return;
        }

        match point_for_position.as_valid() {
            Some(point) => {
                let trigger_point = TriggerPoint::Text(
                    snapshot
                        .buffer_snapshot()
                        .anchor_before(point.to_offset(&snapshot.display_snapshot, Bias::Left)),
                );

                show_link_definition(modifiers.shift, self, trigger_point, snapshot, window, cx);
            }
            None => {
                self.update_inlay_link_and_hover_points(
                    snapshot,
                    point_for_position,
                    mouse_position,
                    hovered_link_modifier,
                    modifiers.shift,
                    window,
                    cx,
                );
            }
        }
    }

    pub(crate) fn hide_hovered_link(&mut self, cx: &mut Context<Self>) {
        self.hovered_link_state.take();
        self.clear_highlights(HighlightKey::HoveredLinkState, cx);
    }

    pub(crate) fn handle_click_hovered_link(
        &mut self,
        point: PointForPosition,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let reveal_task = self.cmd_click_reveal_task(point, modifiers, window, cx);
        cx.spawn_in(window, async move |editor, cx| {
            let definition_revealed = reveal_task.await.log_err().unwrap_or(Navigated::No);
            let find_references = editor
                .update_in(cx, |editor, window, cx| {
                    if definition_revealed == Navigated::Yes {
                        return None;
                    }
                    match EditorSettings::get_global(cx).go_to_definition_fallback {
                        GoToDefinitionFallback::None => None,
                        GoToDefinitionFallback::FindAllReferences => {
                            editor.find_all_references(&FindAllReferences::default(), window, cx)
                        }
                    }
                })
                .ok()
                .flatten();
            if let Some(find_references) = find_references {
                find_references.await.log_err();
            }
        })
        .detach();
    }

    pub fn scroll_hover(
        &mut self,
        amount: ScrollAmount,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let selection = self.selections.newest_anchor().head();
        let snapshot = self.snapshot(window, cx);

        if let Some(popover) = self.hover_state.info_popovers.iter().find(|popover| {
            popover
                .symbol_range
                .point_within_range(&TriggerPoint::Text(selection), &snapshot)
        }) {
            popover.scroll(amount, window, cx);
            true
        } else if let Some(context_menu) = self.context_menu.borrow_mut().as_mut() {
            context_menu.scroll_aside(amount, window, cx);
            true
        } else {
            false
        }
    }

    fn cmd_click_reveal_task(
        &mut self,
        point: PointForPosition,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<anyhow::Result<Navigated>> {
        if let Some(hovered_link_state) = self.hovered_link_state.take() {
            self.hide_hovered_link(cx);
            if !hovered_link_state.links.is_empty() {
                if !self.focus_handle.is_focused(window) {
                    window.focus(&self.focus_handle, cx);
                }

                // exclude links pointing back to the current anchor
                let current_position = point
                    .next_valid
                    .to_point(&self.snapshot(window, cx).display_snapshot);
                let Some((buffer, anchor)) = self
                    .buffer()
                    .read(cx)
                    .text_anchor_for_position(current_position, cx)
                else {
                    return Task::ready(Ok(Navigated::No));
                };
                let Some(multi_buffer_anchor) = self
                    .buffer()
                    .read(cx)
                    .snapshot(cx)
                    .anchor_in_excerpt(anchor)
                else {
                    return Task::ready(Ok(Navigated::No));
                };
                let links = hovered_link_state
                    .links
                    .into_iter()
                    .filter(|link| {
                        if let HoverLink::Text(location) = link {
                            exclude_link_to_position(&buffer, &anchor, location, cx)
                        } else {
                            true
                        }
                    })
                    .collect();
                let nav_entry = self.navigation_entry(multi_buffer_anchor, cx);
                let split = Self::is_alt_pressed(&modifiers, cx);
                let navigate_task =
                    self.navigate_to_hover_links(None, links, nav_entry, split, window, cx);
                self.select(SelectPhase::End, window, cx);
                return navigate_task;
            }
        }

        // We don't have the correct kind of link cached, set the selection on
        // click and immediately trigger GoToDefinition.
        self.select(
            SelectPhase::Begin {
                position: point.next_valid,
                add: false,
                click_count: 1,
            },
            window,
            cx,
        );

        let navigate_task = if point.as_valid().is_some() {
            let split = Self::is_alt_pressed(&modifiers, cx);
            match (modifiers.shift, split) {
                (true, true) => {
                    self.go_to_type_definition_split(&GoToTypeDefinitionSplit, window, cx)
                }
                (true, false) => self.go_to_type_definition(&GoToTypeDefinition, window, cx),
                (false, true) => self.go_to_definition_split(&GoToDefinitionSplit, window, cx),
                (false, false) => self.go_to_definition(&GoToDefinition, window, cx),
            }
        } else {
            Task::ready(Ok(Navigated::No))
        };
        self.select(SelectPhase::End, window, cx);
        navigate_task
    }
}

pub(crate) fn find_url(
    buffer: &Entity<language::Buffer>,
    position: text::Anchor,
    cx: &AsyncWindowContext,
) -> Option<(Range<text::Anchor>, String)> {
    const LIMIT: usize = 2048;

    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());

    let offset = position.to_offset(&snapshot);
    let mut token_start = offset;
    let mut token_end = offset;
    let mut found_start = false;
    let mut found_end = false;

    for ch in snapshot.reversed_chars_at(offset).take(LIMIT) {
        if ch.is_whitespace() {
            found_start = true;
            break;
        }
        token_start -= ch.len_utf8();
    }
    // Check if we didn't find the starting whitespace or if we didn't reach the start of the buffer
    if !found_start && token_start != 0 {
        return None;
    }

    for ch in snapshot
        .chars_at(offset)
        .take(LIMIT - (offset - token_start))
    {
        if ch.is_whitespace() {
            found_end = true;
            break;
        }
        token_end += ch.len_utf8();
    }
    // Check if we didn't find the ending whitespace or if we read more or equal than LIMIT
    // which at this point would happen only if we reached the end of buffer
    if !found_end && (token_end - token_start >= LIMIT) {
        return None;
    }

    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    let input = snapshot
        .text_for_range(token_start..token_end)
        .collect::<String>();

    let relative_offset = offset - token_start;
    for link in finder.links(&input) {
        if link.start() <= relative_offset && link.end() >= relative_offset {
            let range = snapshot.anchor_before(token_start + link.start())
                ..snapshot.anchor_after(token_start + link.end());
            return Some((range, link.as_str().to_string()));
        }
    }
    None
}

pub(crate) fn find_url_from_range(
    buffer: &Entity<language::Buffer>,
    range: Range<text::Anchor>,
    cx: &AsyncWindowContext,
) -> Option<String> {
    const LIMIT: usize = 2048;

    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());

    let start_offset = range.start.to_offset(&snapshot);
    let end_offset = range.end.to_offset(&snapshot);

    let mut token_start = start_offset.min(end_offset);
    let mut token_end = start_offset.max(end_offset);

    let range_len = token_end - token_start;

    if range_len >= LIMIT {
        return None;
    }

    // Skip leading whitespace
    for ch in snapshot.chars_at(token_start).take(range_len) {
        if !ch.is_whitespace() {
            break;
        }
        token_start += ch.len_utf8();
    }

    // Skip trailing whitespace
    for ch in snapshot.reversed_chars_at(token_end).take(range_len) {
        if !ch.is_whitespace() {
            break;
        }
        token_end -= ch.len_utf8();
    }

    if token_start >= token_end {
        return None;
    }

    let text = snapshot
        .text_for_range(token_start..token_end)
        .collect::<String>();

    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);

    if let Some(link) = finder.links(&text).next()
        && link.start() == 0
        && link.end() == text.len()
    {
        return Some(link.as_str().to_string());
    }

    None
}

#[cfg(test)]
#[path = "hover_links/tests.rs"]
mod tests;

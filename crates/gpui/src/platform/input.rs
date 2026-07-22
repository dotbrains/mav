#![expect(missing_docs)]

use super::*;

pub struct PlatformInputHandler {
    cx: AsyncWindowContext,
    handler: Box<dyn InputHandler>,
}

#[expect(missing_docs)]
#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
impl PlatformInputHandler {
    pub fn new(cx: AsyncWindowContext, handler: Box<dyn InputHandler>) -> Self {
        Self { cx, handler }
    }

    pub fn selected_text_range(&mut self, ignore_disabled_input: bool) -> Option<UTF16Selection> {
        self.cx
            .update(|window, cx| {
                self.handler
                    .selected_text_range(ignore_disabled_input, window, cx)
            })
            .ok()
            .flatten()
    }

    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub fn marked_text_range(&mut self) -> Option<Range<usize>> {
        self.cx
            .update(|window, cx| self.handler.marked_text_range(window, cx))
            .ok()
            .flatten()
    }

    #[cfg_attr(
        any(target_os = "linux", target_os = "freebsd", target_os = "windows"),
        allow(dead_code)
    )]
    pub fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
    ) -> Option<String> {
        self.cx
            .update(|window, cx| {
                self.handler
                    .text_for_range(range_utf16, adjusted, window, cx)
            })
            .ok()
            .flatten()
    }

    pub fn replace_text_in_range(&mut self, replacement_range: Option<Range<usize>>, text: &str) {
        self.cx
            .update(|window, cx| {
                self.handler
                    .replace_text_in_range(replacement_range, text, window, cx);
            })
            .ok();
    }

    pub fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
    ) {
        self.cx
            .update(|window, cx| {
                self.handler.replace_and_mark_text_in_range(
                    range_utf16,
                    new_text,
                    new_selected_range,
                    window,
                    cx,
                )
            })
            .ok();
    }

    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub fn unmark_text(&mut self) {
        self.cx
            .update(|window, cx| self.handler.unmark_text(window, cx))
            .ok();
    }

    pub fn bounds_for_range(&mut self, range_utf16: Range<usize>) -> Option<Bounds<Pixels>> {
        self.cx
            .update(|window, cx| self.handler.bounds_for_range(range_utf16, window, cx))
            .ok()
            .flatten()
    }

    #[allow(dead_code)]
    pub fn apple_press_and_hold_enabled(&mut self) -> bool {
        self.handler.apple_press_and_hold_enabled()
    }

    pub fn dispatch_input(&mut self, input: &str, window: &mut Window, cx: &mut App) {
        self.handler.replace_text_in_range(None, input, window, cx);
    }

    pub fn compute_ime_candidate_bounds(
        marked_range: Option<Range<usize>>,
        selection: &UTF16Selection,
        mut bounds_for_range: impl FnMut(Range<usize>) -> Option<Bounds<Pixels>>,
    ) -> Option<Bounds<Pixels>> {
        if let Some(marked_range) = marked_range {
            // Default to the start of the marked (composing) range.
            let mut line_start = marked_range.start;

            // Walk backward from the caret looking for a line break. A change in
            // the Y coordinate means we crossed into the previous visual line, so
            // the line start is one position after the break point.
            let caret = selection.range.end;
            if let Some(caret_bounds) = bounds_for_range(caret..caret) {
                for i in (marked_range.start..caret).rev() {
                    if let Some(b) = bounds_for_range(i..i) {
                        if (b.origin.y - caret_bounds.origin.y).abs() > px(0.1) {
                            line_start = i + 1;
                            break;
                        }
                    }
                }
            }
            bounds_for_range(line_start..line_start)
        } else {
            // No active composition — use the selection endpoint.
            let offset = if selection.reversed {
                selection.range.start
            } else {
                selection.range.end
            };
            bounds_for_range(offset..offset)
        }
    }

    pub fn selected_bounds(&mut self, window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
        let marked_range = self.handler.marked_text_range(window, cx);
        let selection = self.handler.selected_text_range(true, window, cx)?;
        Self::compute_ime_candidate_bounds(marked_range, &selection, |range| {
            self.handler.bounds_for_range(range, window, cx)
        })
    }

    pub fn ime_candidate_bounds(&mut self) -> Option<Bounds<Pixels>> {
        let marked_range = self.marked_text_range();
        let selection = self.selected_text_range(true)?;
        Self::compute_ime_candidate_bounds(marked_range, &selection, |range| {
            self.bounds_for_range(range)
        })
    }

    #[allow(unused)]
    pub fn character_index_for_point(&mut self, point: Point<Pixels>) -> Option<usize> {
        self.cx
            .update(|window, cx| self.handler.character_index_for_point(point, window, cx))
            .ok()
            .flatten()
    }

    #[allow(dead_code)]
    pub fn accepts_text_input(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.handler.accepts_text_input(window, cx)
    }

    #[allow(dead_code)]
    pub fn query_accepts_text_input(&mut self) -> bool {
        self.cx
            .update(|window, cx| self.handler.accepts_text_input(window, cx))
            .unwrap_or(true)
    }

    #[allow(dead_code)]
    pub fn query_prefers_ime_for_printable_keys(&mut self) -> bool {
        self.cx
            .update(|window, cx| self.handler.prefers_ime_for_printable_keys(window, cx))
            .unwrap_or(false)
    }
}

/// A struct representing a selection in a text buffer, in UTF16 characters.
/// This is different from a range because the head may be before the tail.
#[derive(Debug)]
pub struct UTF16Selection {
    /// The range of text in the document this selection corresponds to
    /// in UTF16 characters.
    pub range: Range<usize>,
    /// Whether the head of this selection is at the start (true), or end (false)
    /// of the range
    pub reversed: bool,
}

/// Mav's interface for handling text input from the platform's IME system
/// This is currently a 1:1 exposure of the NSTextInputClient API:
///
/// <https://developer.apple.com/documentation/appkit/nstextinputclient>
pub trait InputHandler: 'static {
    /// Get the range of the user's currently selected text, if any
    /// Corresponds to [selectedRange()](https://developer.apple.com/documentation/appkit/nstextinputclient/1438242-selectedrange)
    ///
    /// Return value is in terms of UTF-16 characters, from 0 to the length of the document
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection>;

    /// Get the range of the currently marked text, if any
    /// Corresponds to [markedRange()](https://developer.apple.com/documentation/appkit/nstextinputclient/1438250-markedrange)
    ///
    /// Return value is in terms of UTF-16 characters, from 0 to the length of the document
    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>>;

    /// Get the text for the given document range in UTF-16 characters
    /// Corresponds to [attributedSubstring(forProposedRange: actualRange:)](https://developer.apple.com/documentation/appkit/nstextinputclient/1438238-attributedsubstring)
    ///
    /// range_utf16 is in terms of UTF-16 characters
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String>;

    /// Replace the text in the given document range with the given text
    /// Corresponds to [insertText(_:replacementRange:)](https://developer.apple.com/documentation/appkit/nstextinputclient/1438258-inserttext)
    ///
    /// replacement_range is in terms of UTF-16 characters
    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    );

    /// Replace the text in the given document range with the given text,
    /// and mark the given text as part of an IME 'composing' state
    /// Corresponds to [setMarkedText(_:selectedRange:replacementRange:)](https://developer.apple.com/documentation/appkit/nstextinputclient/1438246-setmarkedtext)
    ///
    /// range_utf16 is in terms of UTF-16 characters
    /// new_selected_range is in terms of UTF-16 characters
    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    );

    /// Remove the IME 'composing' state from the document
    /// Corresponds to [unmarkText()](https://developer.apple.com/documentation/appkit/nstextinputclient/1438239-unmarktext)
    fn unmark_text(&mut self, window: &mut Window, cx: &mut App);

    /// Get the bounds of the given document range in screen coordinates
    /// Corresponds to [firstRect(forCharacterRange:actualRange:)](https://developer.apple.com/documentation/appkit/nstextinputclient/1438240-firstrect)
    ///
    /// This is used for positioning the IME candidate window
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>>;

    /// Get the character offset for the given point in terms of UTF16 characters
    ///
    /// Corresponds to [characterIndexForPoint:](https://developer.apple.com/documentation/appkit/nstextinputclient/characterindex(for:))
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize>;

    /// Allows a given input context to opt into getting raw key repeats instead of
    /// sending these to the platform.
    /// TODO: Ideally we should be able to set ApplePressAndHoldEnabled in NSUserDefaults
    /// (which is how iTerm does it) but it doesn't seem to work for me.
    #[allow(dead_code)]
    fn apple_press_and_hold_enabled(&mut self) -> bool {
        true
    }

    /// Returns whether this handler is accepting text input to be inserted.
    fn accepts_text_input(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }

    /// Returns whether printable keys should be routed to the IME before keybinding
    /// matching when a non-ASCII input source (e.g. Japanese, Korean, Chinese IME)
    /// is active. This prevents multi-stroke keybindings like `jj` from intercepting
    /// keys that the IME should compose.
    ///
    /// Defaults to `false`. The editor overrides this based on whether it expects
    /// character input (e.g. Vim insert mode returns `true`, normal mode returns `false`).
    /// The terminal keeps the default `false` so that raw keys reach the terminal process.
    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        false
    }
}

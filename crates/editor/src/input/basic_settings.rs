use super::*;

impl Editor {
    pub fn set_input_enabled(&mut self, input_enabled: bool) {
        self.input_enabled = input_enabled;
    }

    pub fn set_expects_character_input(&mut self, expects_character_input: bool) {
        self.expects_character_input = expects_character_input;
    }

    pub fn set_autoindent(&mut self, autoindent: bool) {
        if autoindent {
            self.autoindent_mode = Some(AutoindentMode::EachLine);
        } else {
            self.autoindent_mode = None;
        }
    }

    pub fn set_use_autoclose(&mut self, autoclose: bool) {
        self.use_autoclose = autoclose;
    }

    pub fn replay_insert_event(
        &mut self,
        text: &str,
        relative_utf16_range: Option<Range<isize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            cx.emit(EditorEvent::InputIgnored { text: text.into() });
            return;
        }

        cx.emit(EditorEvent::InputHandled {
            utf16_range_to_replace: relative_utf16_range.clone(),
            text: text.into(),
        });

        if let Some(relative_utf16_range) = relative_utf16_range {
            let selections = self
                .selections
                .all::<MultiBufferOffsetUtf16>(&self.display_snapshot(cx));
            self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                let new_ranges = selections.into_iter().map(|range| {
                    let start = MultiBufferOffsetUtf16(OffsetUtf16(
                        range
                            .head()
                            .0
                            .0
                            .saturating_add_signed(relative_utf16_range.start),
                    ));
                    let end = MultiBufferOffsetUtf16(OffsetUtf16(
                        range
                            .head()
                            .0
                            .0
                            .saturating_add_signed(relative_utf16_range.end),
                    ));
                    start..end
                });
                s.select_ranges(new_ranges);
            });
        }

        self.handle_input(text, window, cx);
    }
}

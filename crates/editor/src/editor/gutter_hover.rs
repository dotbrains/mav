use super::*;

#[derive(Clone, Copy)]
enum GutterHoverIntent {
    SetBookmark,
    SetBreakpoint,
}

impl GutterHoverIntent {
    fn as_str(&self) -> &'static str {
        match self {
            GutterHoverIntent::SetBookmark => "Set bookmark",
            GutterHoverIntent::SetBreakpoint => "Set breakpoint",
        }
    }

    fn icon(&self) -> ui::IconName {
        match self {
            GutterHoverIntent::SetBookmark => ui::IconName::Bookmark,
            GutterHoverIntent::SetBreakpoint => ui::IconName::DebugBreakpoint,
        }
    }

    fn color(&self) -> Color {
        match self {
            GutterHoverIntent::SetBookmark => Color::Info,
            GutterHoverIntent::SetBreakpoint => Color::Hint,
        }
    }

    fn secondary_and_options(&self) -> String {
        let alt_as_text = gpui::Keystroke {
            modifiers: Modifiers::secondary_key(),
            ..Default::default()
        };
        match self {
            GutterHoverIntent::SetBookmark => {
                format!("{alt_as_text}-click to add a breakpoint\nright-click for more options")
            }
            GutterHoverIntent::SetBreakpoint => {
                format!("{alt_as_text}-click to add a bookmark\nright-click for more options")
            }
        }
    }
}

impl Editor {
    pub(super) fn render_gutter_hover_button(
        &self,
        position: Anchor,
        row: DisplayRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> IconButton {
        let gutter_settings = EditorSettings::get_global(cx).gutter;
        let show_bookmarks = self.show_bookmarks.unwrap_or(gutter_settings.bookmarks);
        let show_breakpoints = self.show_breakpoints.unwrap_or(gutter_settings.breakpoints);

        let [primary, secondary] = match [show_breakpoints, show_bookmarks] {
            [true, true] => [
                GutterHoverIntent::SetBreakpoint,
                GutterHoverIntent::SetBookmark,
            ],
            [true, false] => [GutterHoverIntent::SetBreakpoint; 2],
            [false, true] => [GutterHoverIntent::SetBookmark; 2],
            [false, false] => {
                log::error!("Trying to place gutter_hover without anything enabled!!");
                [GutterHoverIntent::SetBookmark; 2]
            }
        };

        let intent = if window.modifiers().secondary() {
            secondary
        } else {
            primary
        };

        let focus_handle = self.focus_handle.clone();
        let has_context_menu = self.has_mouse_context_menu();
        IconButton::new(("add_breakpoint_button", row.0 as usize), intent.icon())
            .icon_size(IconSize::XSmall)
            .size(ui::ButtonSize::None)
            .icon_color(intent.color())
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener({
                move |editor, _: &ClickEvent, window, cx| {
                    window.focus(&editor.focus_handle(cx), cx);
                    let intent = if window.modifiers().secondary() {
                        secondary
                    } else {
                        primary
                    };

                    match intent {
                        GutterHoverIntent::SetBookmark => {
                            editor.toggle_bookmark_at_row(row, window, cx)
                        }
                        GutterHoverIntent::SetBreakpoint => editor.edit_breakpoint_at_anchor(
                            position,
                            Breakpoint::new_standard(),
                            BreakpointEditAction::Toggle,
                            cx,
                        ),
                    }
                }
            }))
            .on_right_click(cx.listener(move |editor, event: &ClickEvent, window, cx| {
                editor.set_gutter_context_menu(row, Some(position), event.position(), window, cx);
            }))
            .when(!has_context_menu, |button| {
                button.tooltip(move |_window, cx| {
                    Tooltip::with_meta_in(
                        intent.as_str(),
                        Some(&ToggleBreakpoint),
                        intent.secondary_and_options(),
                        &focus_handle,
                        cx,
                    )
                })
            })
    }
}

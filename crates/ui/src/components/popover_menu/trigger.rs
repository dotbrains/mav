use gpui::{App, Window};

use crate::prelude::*;

impl<T: Clickable> Clickable for gpui::AnimationElement<T>
where
    T: Clickable + 'static,
{
    fn on_click(
        self,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.map_element(|e| e.on_click(handler))
    }

    fn cursor_style(self, cursor_style: gpui::CursorStyle) -> Self {
        self.map_element(|e| e.cursor_style(cursor_style))
    }
}

impl<T: Toggleable> Toggleable for gpui::AnimationElement<T>
where
    T: Toggleable + 'static,
{
    fn toggle_state(self, selected: bool) -> Self {
        self.map_element(|e| e.toggle_state(selected))
    }
}

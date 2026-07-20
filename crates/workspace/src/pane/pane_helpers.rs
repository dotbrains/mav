use super::*;

pub(crate) fn render_toggle_zoom_button(pane: &Pane, cx: &mut Context<Pane>) -> IconButton {
    let zoomed = pane.is_zoomed();
    IconButton::new("toggle_zoom", IconName::Maximize)
        .icon_size(IconSize::Small)
        .toggle_state(zoomed)
        .selected_icon(IconName::Minimize)
        .on_click(cx.listener(|pane, _, window, cx| {
            pane.toggle_zoom(&crate::ToggleZoom, window, cx);
        }))
        .tooltip(move |_window, cx| {
            Tooltip::for_action(if zoomed { "Zoom Out" } else { "Zoom In" }, &ToggleZoom, cx)
        })
}

pub(crate) fn dirty_message_for(buffer_path: Option<ProjectPath>, path_style: PathStyle) -> String {
    let path = buffer_path
        .as_ref()
        .and_then(|p| {
            let path = p.path.display(path_style);
            if path.is_empty() { None } else { Some(path) }
        })
        .unwrap_or("This buffer".into());
    let path = truncate_and_remove_front(&path, 80);
    format!("{path} contains unsaved edits. Do you want to save it?")
}

pub fn tab_details(items: &[Box<dyn ItemHandle>], _window: &Window, cx: &App) -> Vec<usize> {
    util::disambiguate::compute_disambiguation_details(items, |item, detail| {
        item.tab_content_text(detail, cx)
    })
}

pub fn render_item_indicator(item: Box<dyn ItemHandle>, cx: &App) -> Option<Indicator> {
    maybe!({
        let indicator_color = match (item.has_conflict(cx), item.is_dirty(cx)) {
            (true, _) => Color::Warning,
            (_, true) => Color::Accent,
            (false, false) => return None,
        };

        Some(Indicator::dot().color(indicator_color))
    })
}

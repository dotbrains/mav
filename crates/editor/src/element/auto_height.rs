use super::*;

/// Shared between prepaint and auto-height editors so they compute wrap widths consistently.
pub(super) fn calculate_wrap_width(
    soft_wrap: SoftWrap,
    editor_width: Pixels,
    em_width: Pixels,
) -> Option<Pixels> {
    let wrap_width_for = |column: u32| (column as f32 * em_width).ceil();

    match soft_wrap {
        SoftWrap::GitDiff => None,
        SoftWrap::None => Some(wrap_width_for(MAX_LINE_LEN as u32 / 2)),
        SoftWrap::EditorWidth => Some(editor_width),
        SoftWrap::Bounded(column) => Some(editor_width.min(wrap_width_for(column))),
    }
}

pub(super) fn compute_auto_height_layout(
    editor: &mut Editor,
    min_lines: usize,
    max_lines: Option<usize>,
    known_dimensions: Size<Option<Pixels>>,
    available_width: AvailableSpace,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Option<Size<Pixels>> {
    let width = known_dimensions.width.or({
        if let AvailableSpace::Definite(available_width) = available_width {
            Some(available_width)
        } else {
            None
        }
    })?;
    if let Some(height) = known_dimensions.height {
        return Some(size(width, height));
    }

    let style = editor.style.as_ref().unwrap();
    let font_id = window.text_system().resolve_font(&style.text.font());
    let font_size = style.text.font_size.to_pixels(window.rem_size());
    let line_height = style.text.line_height_in_pixels(window.rem_size());
    let em_width = window.text_system().em_width(font_id, font_size).unwrap();

    let mut snapshot = editor.snapshot(window, cx);
    let gutter_dimensions = snapshot.gutter_dimensions(font_id, font_size, style, window, cx);

    editor.gutter_dimensions = gutter_dimensions;
    let text_width = width - gutter_dimensions.width;
    let overscroll = size(em_width, px(0.));

    let editor_width = text_width - gutter_dimensions.margin - overscroll.width - em_width;
    let wrap_width = calculate_wrap_width(editor.soft_wrap_mode(cx), editor_width, em_width)
        .map(|width| width.min(editor_width));
    if wrap_width.is_some() && editor.set_wrap_width(wrap_width, cx) {
        snapshot = editor.snapshot(window, cx);
    }

    let scroll_height = (snapshot.max_point().row().next_row().0 as f32) * line_height;

    let min_height = line_height * min_lines as f32;
    let content_height = scroll_height.max(min_height);

    let final_height = if let Some(max_lines) = max_lines {
        let max_height = line_height * max_lines as f32;
        content_height.min(max_height)
    } else {
        content_height
    };

    Some(size(width, final_height))
}

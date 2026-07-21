use super::*;
use crate::{
    Editor, HighlightKey, MultiBuffer, NavigationOverlayKey, NavigationOverlayLabel,
    NavigationTargetOverlay, SelectionEffects,
    display_map::{BlockPlacement, BlockProperties},
    editor_tests::{init_test, update_test_language_settings},
    element::navigation_overlay::NavigationLabelLayout,
};
use gpui::{TestAppContext, VisualTestContext};
use language::{Buffer, language_settings, tree_sitter_python};
use log::info;
use rand::{RngCore, rngs::StdRng};
use std::num::NonZeroU32;
use util::test::sample_text;

enum PrimaryNavigationOverlay {}

const PRIMARY_NAVIGATION_OVERLAY_KEY: NavigationOverlayKey =
    NavigationOverlayKey::unique::<PrimaryNavigationOverlay>();

fn navigation_overlay(
    label_text: &'static str,
    target_range: Range<Anchor>,
    covered_text_range: Option<Range<Anchor>>,
) -> NavigationTargetOverlay {
    NavigationTargetOverlay {
        target_range,
        label: NavigationOverlayLabel {
            text: SharedString::from(label_text),
            text_color: Hsla::black(),
            x_offset: Pixels::ZERO,
            scale_factor: 1.0,
        },
        covered_text_range,
    }
}

fn navigation_label_layouts(state: &EditorLayout) -> Vec<&NavigationLabelLayout> {
    state
        .navigation_overlay_paint_commands
        .iter()
        .map(|command| match command {
            NavigationOverlayPaintCommand::Label(label) => label,
        })
        .collect()
}

fn placeholder_hitbox() -> Hitbox {
    use gpui::HitboxId;
    let zero_bounds = Bounds {
        origin: point(Pixels::ZERO, Pixels::ZERO),
        size: Size {
            width: Pixels::ZERO,
            height: Pixels::ZERO,
        },
    };

    Hitbox {
        id: HitboxId::placeholder(),
        bounds: zero_bounds,
        content_mask: ContentMask::new(zero_bounds),
        behavior: HitboxBehavior::Normal,
    }
}

fn test_gutter(line_height: Pixels, snapshot: &EditorSnapshot) -> Gutter<'_> {
    const DIMENSIONS: GutterDimensions = GutterDimensions {
        left_padding: Pixels::ZERO,
        right_padding: Pixels::ZERO,
        width: px(30.0),
        margin: Pixels::ZERO,
        git_blame_entries_width: None,
    };
    const EMPTY_ROW_INFO: RowInfo = RowInfo {
        buffer_id: None,
        buffer_row: None,
        multibuffer_row: None,
        diff_status: None,
        expand_info: None,
        wrapped_buffer_row: None,
    };

    const fn row_info(row: u32) -> RowInfo {
        RowInfo {
            buffer_row: Some(row),
            ..EMPTY_ROW_INFO
        }
    }

    const ROW_INFOS: [RowInfo; 6] = [
        row_info(0),
        row_info(1),
        row_info(2),
        row_info(3),
        row_info(4),
        row_info(5),
    ];

    let hitbox = Box::leak(Box::new(placeholder_hitbox()));
    Gutter {
        line_height,
        range: DisplayRow(0)..DisplayRow(6),
        scroll_position: gpui::Point::default(),
        dimensions: &DIMENSIONS,
        hitbox,
        snapshot: snapshot,
        row_infos: &ROW_INFOS,
    }
}

fn collect_invisibles_from_new_editor(
    cx: &mut TestAppContext,
    editor_mode: EditorMode,
    input_text: &str,
    editor_width: Pixels,
    show_line_numbers: bool,
) -> Vec<Invisible> {
    info!(
        "Creating editor with mode {editor_mode:?}, width {}px and text '{input_text}'",
        f32::from(editor_width)
    );
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(input_text, cx);
        Editor::new(editor_mode, buffer, None, window, cx)
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let editor = window.root(cx).unwrap();

    let style = editor.update(cx, |editor, cx| editor.style(cx).clone());
    window
        .update(cx, |editor, _, cx| {
            editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
            editor.set_wrap_width(Some(editor_width), cx);
            editor.set_show_line_numbers(show_line_numbers, cx);
        })
        .unwrap();
    let (_, state) = cx.draw(
        point(px(500.), px(500.)),
        size(px(500.), px(500.)),
        |_, _| EditorElement::new(&editor, style),
    );
    state
        .position_map
        .line_layouts
        .iter()
        .flat_map(|line_with_invisibles| &line_with_invisibles.invisibles)
        .cloned()
        .collect()
}

fn generate_test_run(len: usize, color: Hsla) -> TextRun {
    TextRun {
        len,
        color,
        ..Default::default()
    }
}

mod backgrounds;
mod invisibles;
mod line_numbers;
mod navigation;
mod runs_and_spacers;
mod soft_wrap;
mod visual_blocks;

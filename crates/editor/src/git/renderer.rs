use super::*;

pub fn set_blame_renderer(renderer: impl BlameRenderer + 'static, cx: &mut App) {
    cx.set_global(GlobalBlameRenderer(Arc::new(renderer)));
}
pub(super) fn render_diff_hunk_controls(
    row: u32,
    status: &DiffHunkStatus,
    hunk_range: Range<Anchor>,
    is_created_file: bool,
    line_height: Pixels,
    editor: &Entity<Editor>,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let show_stage_restore = ProjectSettings::get_global(cx)
        .git
        .show_stage_restore_buttons;

    h_flex()
        .h(line_height)
        .mr_1()
        .gap_1()
        .px_0p5()
        .pb_1()
        .border_x_1()
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        .rounded_b_lg()
        .bg(cx.theme().colors().editor_background)
        .gap_1()
        .block_mouse_except_scroll()
        .shadow_md()
        .when(show_stage_restore, |el| {
            el.child(if status.has_secondary_hunk() {
                Button::new(("stage", row as u64), "Stage")
                    .alpha(if status.is_pending() { 0.66 } else { 1.0 })
                    .tooltip({
                        let focus_handle = editor.focus_handle(cx);
                        move |_window, cx| {
                            Tooltip::for_action_in(
                                "Stage Hunk",
                                &::git::ToggleStaged,
                                &focus_handle,
                                cx,
                            )
                        }
                    })
                    .on_click({
                        let editor = editor.clone();
                        move |_event, _window, cx| {
                            editor.update(cx, |editor, cx| {
                                editor.stage_or_unstage_diff_hunks(
                                    true,
                                    vec![hunk_range.start..hunk_range.start],
                                    cx,
                                );
                            });
                        }
                    })
            } else {
                Button::new(("unstage", row as u64), "Unstage")
                    .alpha(if status.is_pending() { 0.66 } else { 1.0 })
                    .tooltip({
                        let focus_handle = editor.focus_handle(cx);
                        move |_window, cx| {
                            Tooltip::for_action_in(
                                "Unstage Hunk",
                                &::git::ToggleStaged,
                                &focus_handle,
                                cx,
                            )
                        }
                    })
                    .on_click({
                        let editor = editor.clone();
                        move |_event, _window, cx| {
                            editor.update(cx, |editor, cx| {
                                editor.stage_or_unstage_diff_hunks(
                                    false,
                                    vec![hunk_range.start..hunk_range.start],
                                    cx,
                                );
                            });
                        }
                    })
            })
        })
        .when(show_stage_restore, |el| {
            el.child(
                Button::new(("restore", row as u64), "Restore")
                    .tooltip({
                        let focus_handle = editor.focus_handle(cx);
                        move |_window, cx| {
                            Tooltip::for_action_in(
                                "Restore Hunk",
                                &::git::Restore,
                                &focus_handle,
                                cx,
                            )
                        }
                    })
                    .on_click({
                        let editor = editor.clone();
                        move |_event, window, cx| {
                            editor.update(cx, |editor, cx| {
                                let snapshot = editor.snapshot(window, cx);
                                let point = hunk_range.start.to_point(&snapshot.buffer_snapshot());
                                editor.restore_hunks_in_ranges(vec![point..point], window, cx);
                            });
                        }
                    })
                    .disabled(is_created_file),
            )
        })
        .when(
            !editor.read(cx).buffer().read(cx).all_diff_hunks_expanded(),
            |el| {
                el.child(
                    IconButton::new(("next-hunk", row as u64), IconName::ArrowDown)
                        .shape(IconButtonShape::Square)
                        .icon_size(IconSize::Small)
                        // .disabled(!has_multiple_hunks)
                        .tooltip({
                            let focus_handle = editor.focus_handle(cx);
                            move |_window, cx| {
                                Tooltip::for_action_in("Next Hunk", &GoToHunk, &focus_handle, cx)
                            }
                        })
                        .on_click({
                            let editor = editor.clone();
                            move |_event, window, cx| {
                                editor.update(cx, |editor, cx| {
                                    let snapshot = editor.snapshot(window, cx);
                                    let position =
                                        hunk_range.end.to_point(&snapshot.buffer_snapshot());
                                    editor.go_to_hunk_before_or_after_position(
                                        &snapshot,
                                        position,
                                        Direction::Next,
                                        true,
                                        window,
                                        cx,
                                    );
                                    editor.expand_selected_diff_hunks(cx);
                                });
                            }
                        }),
                )
                .child(
                    IconButton::new(("prev-hunk", row as u64), IconName::ArrowUp)
                        .shape(IconButtonShape::Square)
                        .icon_size(IconSize::Small)
                        // .disabled(!has_multiple_hunks)
                        .tooltip({
                            let focus_handle = editor.focus_handle(cx);
                            move |_window, cx| {
                                Tooltip::for_action_in(
                                    "Previous Hunk",
                                    &GoToPreviousHunk,
                                    &focus_handle,
                                    cx,
                                )
                            }
                        })
                        .on_click({
                            let editor = editor.clone();
                            move |_event, window, cx| {
                                editor.update(cx, |editor, cx| {
                                    let snapshot = editor.snapshot(window, cx);
                                    let point =
                                        hunk_range.start.to_point(&snapshot.buffer_snapshot());
                                    editor.go_to_hunk_before_or_after_position(
                                        &snapshot,
                                        point,
                                        Direction::Prev,
                                        true,
                                        window,
                                        cx,
                                    );
                                    editor.expand_selected_diff_hunks(cx);
                                });
                            }
                        }),
                )
            },
        )
        .into_any_element()
}

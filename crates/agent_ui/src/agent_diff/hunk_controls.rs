use super::*;

fn diff_hunk_controls(
    thread: &Entity<AcpThread>,
    workspace: WeakEntity<Workspace>,
) -> editor::RenderDiffHunkControlsFn {
    let thread = thread.clone();

    Arc::new(
        move |row, status, hunk_range, is_created_file, line_height, editor, _, cx| {
            {
                render_diff_hunk_controls(
                    row,
                    status,
                    hunk_range,
                    is_created_file,
                    line_height,
                    &thread,
                    editor,
                    workspace.clone(),
                    cx,
                )
            }
        },
    )
}

fn render_diff_hunk_controls(
    row: u32,
    _status: &DiffHunkStatus,
    hunk_range: Range<editor::Anchor>,
    is_created_file: bool,
    line_height: Pixels,
    thread: &Entity<AcpThread>,
    editor: &Entity<Editor>,
    workspace: WeakEntity<Workspace>,
    cx: &mut App,
) -> AnyElement {
    let editor = editor.clone();
    // Drop shadows render as a dark halo on transparent windows.
    let opaque_window =
        cx.theme().window_background_appearance() == gpui::WindowBackgroundAppearance::Opaque;

    h_flex()
        .h(line_height)
        .mr_0p5()
        .gap_1()
        .px_0p5()
        .pb_1()
        .border_x_1()
        .border_b_1()
        .border_color(cx.theme().colors().border)
        .rounded_b_md()
        .bg(cx.theme().colors().editor_background)
        .gap_1()
        .block_mouse_except_scroll()
        .when(opaque_window, |this| this.shadow_md())
        .children(vec![
            Button::new(("reject", row as u64), "Reject")
                .disabled(is_created_file)
                .key_binding(
                    KeyBinding::for_action_in(&Reject, &editor.read(cx).focus_handle(cx), cx)
                        .map(|kb| kb.size(rems_from_px(12.))),
                )
                .on_click({
                    let editor = editor.clone();
                    let thread = thread.clone();
                    move |_event, window, cx| {
                        editor.update(cx, |editor, cx| {
                            let snapshot = editor.buffer().read(cx).snapshot(cx);
                            reject_edits_in_ranges(
                                editor,
                                &snapshot,
                                &thread,
                                vec![hunk_range.start..hunk_range.start],
                                workspace.clone(),
                                window,
                                cx,
                            );
                        })
                    }
                }),
            Button::new(("keep", row as u64), "Keep")
                .key_binding(
                    KeyBinding::for_action_in(&Keep, &editor.read(cx).focus_handle(cx), cx)
                        .map(|kb| kb.size(rems_from_px(12.))),
                )
                .on_click({
                    let editor = editor.clone();
                    let thread = thread.clone();
                    move |_event, window, cx| {
                        editor.update(cx, |editor, cx| {
                            let snapshot = editor.buffer().read(cx).snapshot(cx);
                            keep_edits_in_ranges(
                                editor,
                                &snapshot,
                                &thread,
                                vec![hunk_range.start..hunk_range.start],
                                window,
                                cx,
                            );
                        });
                    }
                }),
        ])
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

struct AgentDiffAddon;

impl editor::Addon for AgentDiffAddon {
    fn to_any(&self) -> &dyn std::any::Any {
        self
    }

    fn extend_key_context(&self, key_context: &mut gpui::KeyContext, _: &App) {
        key_context.add("agent_diff");
    }
}

use super::*;
use std::cell::Cell;

#[derive(Default)]
pub struct EditorRequestLayoutState {
    // We use prepaint depth to limit the number of times prepaint is
    // called recursively. We need this so that we can update stale
    // data for e.g. block heights in block map.
    prepaint_depth: Rc<Cell<usize>>,
}

impl EditorRequestLayoutState {
    // In ideal conditions we only need one more subsequent prepaint call for resize to take effect.
    // i.e. MAX_PREPAINT_DEPTH = 2, but placing near blocks can expose more lines from below, and
    // we end up querying blocks for those lines too in subsequent renders.
    // Setting MAX_PREPAINT_DEPTH = 3, passes all tests. Just to be on the safe side we set it to 5, so
    // that subsequent shrinking does not lead to incorrect block placing.
    const MAX_PREPAINT_DEPTH: usize = 5;

    pub(super) fn increment_prepaint_depth(&self) -> EditorPrepaintGuard {
        let depth = self.prepaint_depth.get();
        self.prepaint_depth.set(depth + 1);
        EditorPrepaintGuard {
            prepaint_depth: self.prepaint_depth.clone(),
        }
    }

    pub(super) fn has_remaining_prepaint_depth(&self) -> bool {
        self.prepaint_depth.get() < Self::MAX_PREPAINT_DEPTH
    }
}

pub(super) struct EditorPrepaintGuard {
    prepaint_depth: Rc<Cell<usize>>,
}

impl Drop for EditorPrepaintGuard {
    fn drop(&mut self) {
        let depth = self.prepaint_depth.get();
        self.prepaint_depth.set(depth.saturating_sub(1));
    }
}

impl EditorElement {
    pub(super) fn request_layout_impl(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, EditorRequestLayoutState) {
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            self.editor.update(cx, |editor, cx| {
                editor.set_style(self.style.clone(), window, cx);

                let layout_id = match editor.mode {
                    EditorMode::SingleLine => {
                        let rem_size = window.rem_size();
                        let height = self.style.text.line_height_in_pixels(rem_size);
                        let mut style = Style::default();
                        style.size.height = height.into();
                        style.size.width = relative(1.).into();
                        window.request_layout(style, None, cx)
                    }
                    EditorMode::AutoHeight {
                        min_lines,
                        max_lines,
                    } => {
                        let editor_handle = cx.entity();
                        window.request_measured_layout(
                            Style::default(),
                            move |known_dimensions, available_space, window, cx| {
                                editor_handle
                                    .update(cx, |editor, cx| {
                                        compute_auto_height_layout(
                                            editor,
                                            min_lines,
                                            max_lines,
                                            known_dimensions,
                                            available_space.width,
                                            window,
                                            cx,
                                        )
                                    })
                                    .unwrap_or_default()
                            },
                        )
                    }
                    EditorMode::Minimap { .. } => {
                        let mut style = Style::default();
                        style.size.width = relative(1.).into();
                        style.size.height = relative(1.).into();
                        window.request_layout(style, None, cx)
                    }
                    EditorMode::Full {
                        sizing_behavior, ..
                    } => {
                        let mut style = Style::default();
                        style.size.width = relative(1.).into();
                        if sizing_behavior == SizingBehavior::SizeByContent {
                            let snapshot = editor.snapshot(window, cx);
                            let line_height =
                                self.style.text.line_height_in_pixels(window.rem_size());
                            let scroll_height =
                                (snapshot.max_point().row().next_row().0 as f32) * line_height;
                            style.size.height = scroll_height.into();
                        } else {
                            style.size.height = relative(1.).into();
                        }
                        window.request_layout(style, None, cx)
                    }
                };

                (layout_id, EditorRequestLayoutState::default())
            })
        })
    }
}

use std::{
    ops::{Range, RangeInclusive},
    sync::Arc,
};

use buffer_diff::{BufferDiff, BufferDiffSnapshot};
use collections::HashMap;

use gpui::{
    Action, AppContext as _, Entity, EventEmitter, Focusable, Font, Pixels, Subscription,
    WeakEntity, canvas,
};
use itertools::Itertools;
use language::{Buffer, Capability, HighlightedText};
use multi_buffer::{
    Anchor, AnchorRangeExt as _, BufferOffset, ExcerptRange, ExpandExcerptDirection, MultiBuffer,
    MultiBufferDiffHunk, MultiBufferPoint, MultiBufferSnapshot, PathKey,
};
use project::Project;
use rope::Point;
use settings::{DiffViewStyle, SeedQuerySetting, Settings, SettingsStore};
use text::{Bias, BufferId, OffsetRangeExt as _, Patch, ToPoint as _};
use ui::{
    App, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Render,
    Styled as _, Window, div,
};

use crate::{
    display_map::CompanionExcerptPatch,
    element::SplitSide,
    split_editor_view::{SplitEditorState, SplitEditorView},
};
use workspace::{
    ActivatePaneLeft, ActivatePaneRight, Item, ToolbarItemLocation, Workspace,
    item::{ItemBufferKind, ItemEvent, SaveOptions, TabContentParams},
    searchable::{SearchEvent, SearchToken, SearchableItem, SearchableItemHandle},
};

use crate::{
    Autoscroll, Editor, EditorEvent, EditorSettings, RenderDiffHunkControlsFn, ToggleSoftWrap,
    actions::{DisableBreakpoint, EditLogBreakpoint, EnableBreakpoint, ToggleBreakpoint},
    display_map::Companion,
};
use mav_actions::assistant::InlineAssist;

mod patches;
use patches::{
    buffer_range_to_base_text_range, translate_lhs_hunks_to_rhs, translate_lhs_selections_to_rhs,
};
pub(crate) use patches::{patches_for_lhs_range, patches_for_rhs_range};

#[derive(Clone, Copy, PartialEq, Eq, Action, Default)]
#[action(namespace = editor)]
pub struct ToggleSplitDiff;

pub struct SplittableEditor {
    rhs_multibuffer: Entity<MultiBuffer>,
    rhs_editor: Entity<Editor>,
    lhs: Option<LhsEditor>,
    workspace: WeakEntity<Workspace>,
    split_state: Entity<SplitEditorState>,
    searched_side: Option<SplitSide>,
    /// The preferred diff style.
    diff_view_style: DiffViewStyle,
    /// True when the current width is below the minimum threshold for split
    /// mode, regardless of the current diff view style setting.
    too_narrow_for_split: bool,
    last_width: Option<Pixels>,
    _subscriptions: Vec<Subscription>,
}

struct LhsEditor {
    multibuffer: Entity<MultiBuffer>,
    editor: Entity<Editor>,
    was_last_focused: bool,
    _subscriptions: Vec<Subscription>,
}

impl SplittableEditor {
    pub fn rhs_editor(&self) -> &Entity<Editor> {
        &self.rhs_editor
    }

    pub fn lhs_editor(&self) -> Option<&Entity<Editor>> {
        self.lhs.as_ref().map(|s| &s.editor)
    }

    pub fn update_editors(
        &self,
        cx: &mut Context<Self>,
        f: impl Fn(&mut Editor, &mut Context<Editor>),
    ) {
        if let Some(lhs) = &self.lhs {
            lhs.editor.update(cx, &f);
        }
        self.rhs_editor.update(cx, &f);
    }

    pub fn diff_view_style(&self) -> DiffViewStyle {
        self.diff_view_style
    }

    pub fn is_split(&self) -> bool {
        self.lhs.is_some()
    }

    pub fn set_render_diff_hunk_controls(
        &self,
        render_diff_hunk_controls: RenderDiffHunkControlsFn,
        cx: &mut Context<Self>,
    ) {
        self.update_editors(cx, |editor, cx| {
            editor.set_render_diff_hunk_controls(render_diff_hunk_controls.clone(), cx);
        });
    }

    pub fn disable_diff_hunk_controls(&self, cx: &mut Context<Self>) {
        let empty_controls = Arc::new(|_, _: &_, _, _, _, _: &_, _: &mut _, _: &mut _| {
            gpui::Empty.into_any_element()
        });
        self.update_editors(cx, |editor, cx| {
            editor.set_render_diff_hunk_controls(empty_controls.clone(), cx);
        });
    }

    pub fn set_render_diff_hunks_as_unstaged(&self, cx: &mut Context<Self>) {
        self.update_editors(cx, |editor, cx| {
            editor.set_render_diff_hunks_as_unstaged(true, cx);
        });
    }

    fn focused_side(&self) -> SplitSide {
        if let Some(lhs) = &self.lhs
            && lhs.was_last_focused
        {
            SplitSide::Left
        } else {
            SplitSide::Right
        }
    }

    pub fn focused_editor(&self) -> &Entity<Editor> {
        if let Some(lhs) = &self.lhs
            && lhs.was_last_focused
        {
            &lhs.editor
        } else {
            &self.rhs_editor
        }
    }

    pub fn new(
        style: DiffViewStyle,
        rhs_multibuffer: Entity<MultiBuffer>,
        project: Entity<Project>,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let rhs_editor = cx.new(|cx| {
            let mut editor =
                Editor::for_multibuffer(rhs_multibuffer.clone(), Some(project.clone()), window, cx);
            editor.set_expand_all_diff_hunks(cx);
            editor.disable_runnables();
            editor.disable_code_lens(cx);
            editor.disable_inline_diagnostics();
            editor.disable_mouse_wheel_zoom();
            editor.set_minimap_visibility(crate::MinimapVisibility::Disabled, window, cx);
            editor.start_temporary_diff_override();
            editor
        });
        // TODO(split-diff) we might want to tag editor events with whether they came from rhs/lhs
        let subscriptions = vec![
            cx.subscribe(
                &rhs_editor,
                |this, _, event: &EditorEvent, cx| match event {
                    EditorEvent::ExpandExcerptsRequested {
                        excerpt_anchors,
                        lines,
                        direction,
                    } => {
                        this.expand_excerpts(
                            excerpt_anchors.iter().copied(),
                            *lines,
                            *direction,
                            cx,
                        );
                    }
                    _ => cx.emit(event.clone()),
                },
            ),
            cx.subscribe(&rhs_editor, |this, _, event: &SearchEvent, cx| {
                if this.searched_side.is_none() || this.searched_side == Some(SplitSide::Right) {
                    cx.emit(event.clone());
                }
            }),
            cx.observe_global_in::<SettingsStore>(window, move |this, window, cx| {
                let diff_view_style = EditorSettings::get_global(cx).diff_view_style;
                if this.diff_view_style() != diff_view_style {
                    this.toggle_split(&ToggleSplitDiff, window, cx);
                    cx.notify();
                }
            }),
        ];

        let this = cx.weak_entity();
        window.defer(cx, {
            let workspace = workspace.downgrade();
            let rhs_editor = rhs_editor.downgrade();
            move |window, cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        rhs_editor
                            .update(cx, |editor, cx| {
                                editor.added_to_workspace(workspace, window, cx);
                            })
                            .ok();
                    })
                    .ok();
                if style == DiffViewStyle::Split {
                    this.update(cx, |this, cx| {
                        this.split(window, cx);
                    })
                    .ok();
                }
            }
        });
        let split_state = cx.new(|cx| SplitEditorState::new(cx));
        Self {
            diff_view_style: style,
            rhs_editor,
            rhs_multibuffer,
            lhs: None,
            workspace: workspace.downgrade(),
            split_state,
            searched_side: None,
            too_narrow_for_split: false,
            last_width: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.lhs.is_some() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let project = workspace.read(cx).project().clone();
        let all_paths = self.diff_paths(cx);
        if all_paths.is_empty() && !self.rhs_multibuffer.read(cx).is_empty() {
            return;
        }

        let rhs_has_headers = self.rhs_multibuffer.read(cx).snapshot(cx).show_headers();
        let lhs_multibuffer = cx.new(|cx| {
            let mut multibuffer = if !rhs_has_headers {
                MultiBuffer::without_headers(Capability::ReadOnly)
            } else {
                MultiBuffer::new(Capability::ReadOnly)
            };
            multibuffer.set_all_diff_hunks_expanded(cx);
            multibuffer
        });

        let render_diff_hunk_controls = self.rhs_editor.read(cx).render_diff_hunk_controls.clone();
        let render_diff_hunks_as_unstaged = self.rhs_editor.read(cx).render_diff_hunks_as_unstaged;
        let lhs_editor = cx.new(|cx| {
            let mut editor =
                Editor::for_multibuffer(lhs_multibuffer.clone(), Some(project.clone()), window, cx);
            editor.set_render_diff_hunks_as_unstaged(render_diff_hunks_as_unstaged, cx);
            editor.set_number_deleted_lines(true, cx);
            editor.set_delegate_expand_excerpts(true);
            editor.set_delegate_stage_and_restore(true);
            editor.set_delegate_open_excerpts(true);
            editor.set_show_vertical_scrollbar(false, cx);
            editor.disable_lsp_data();
            editor.disable_runnables();
            editor.disable_diagnostics(cx);
            editor.disable_mouse_wheel_zoom();
            editor.set_minimap_visibility(crate::MinimapVisibility::Disabled, window, cx);
            editor
        });

        lhs_editor.update(cx, |editor, cx| {
            editor.set_render_diff_hunk_controls(render_diff_hunk_controls, cx);
        });

        let mut subscriptions = vec![cx.subscribe_in(
            &lhs_editor,
            window,
            |this, _, event: &EditorEvent, window, cx| match event {
                EditorEvent::ExpandExcerptsRequested {
                    excerpt_anchors,
                    lines,
                    direction,
                } => {
                    if let Some(lhs) = &this.lhs {
                        let rhs_snapshot = this.rhs_multibuffer.read(cx).snapshot(cx);
                        let lhs_snapshot = lhs.multibuffer.read(cx).snapshot(cx);
                        let rhs_anchors = excerpt_anchors
                            .iter()
                            .filter_map(|anchor| {
                                let (anchor, lhs_buffer) =
                                    lhs_snapshot.anchor_to_buffer_anchor(*anchor)?;
                                let diff = lhs_snapshot.diff_for_buffer_id(anchor.buffer_id)?;
                                let rhs_buffer_id = diff.buffer_id();
                                let rhs_buffer = rhs_snapshot.buffer_for_id(rhs_buffer_id)?;
                                let rhs_point = diff.base_text_point_to_buffer_point(
                                    anchor.to_point(&lhs_buffer),
                                    &rhs_buffer,
                                );
                                rhs_snapshot.anchor_in_excerpt(rhs_buffer.anchor_before(rhs_point))
                            })
                            .collect::<Vec<_>>();
                        this.expand_excerpts(rhs_anchors.into_iter(), *lines, *direction, cx);
                    }
                }
                EditorEvent::StageOrUnstageRequested { stage, hunks } => {
                    if this.lhs.is_some() {
                        let translated = translate_lhs_hunks_to_rhs(hunks, this, cx);
                        if !translated.is_empty() {
                            let stage = *stage;
                            this.rhs_editor.update(cx, |editor, cx| {
                                let chunk_by = translated.into_iter().chunk_by(|h| h.buffer_id);
                                for (buffer_id, hunks) in &chunk_by {
                                    editor.do_stage_or_unstage(stage, buffer_id, hunks, cx);
                                }
                            });
                        }
                    }
                }
                EditorEvent::RestoreRequested { hunks } => {
                    if this.lhs.is_some() {
                        let translated = translate_lhs_hunks_to_rhs(hunks, this, cx);
                        if !translated.is_empty() {
                            this.rhs_editor.update(cx, |editor, cx| {
                                editor.restore_diff_hunks(translated, cx);
                            });
                        }
                    }
                }
                EditorEvent::OpenExcerptsRequested {
                    selections_by_buffer,
                    split,
                } => {
                    if this.lhs.is_some() {
                        let translated =
                            translate_lhs_selections_to_rhs(selections_by_buffer, this, cx);
                        if !translated.is_empty() {
                            let workspace = this.workspace.clone();
                            let split = *split;
                            Editor::open_buffers_in_workspace(
                                workspace, translated, split, window, cx,
                            );
                        }
                    }
                }
                _ => cx.emit(event.clone()),
            },
        )];

        subscriptions.push(
            cx.subscribe(&lhs_editor, |this, _, event: &SearchEvent, cx| {
                if this.searched_side == Some(SplitSide::Left) {
                    cx.emit(event.clone());
                }
            }),
        );

        let lhs_focus_handle = lhs_editor.read(cx).focus_handle(cx);
        subscriptions.push(
            cx.on_focus_in(&lhs_focus_handle, window, |this, _window, cx| {
                if let Some(lhs) = &mut this.lhs {
                    if !lhs.was_last_focused {
                        lhs.was_last_focused = true;
                        cx.notify();
                    }
                }
            }),
        );

        let rhs_focus_handle = self.rhs_editor.read(cx).focus_handle(cx);
        subscriptions.push(
            cx.on_focus_in(&rhs_focus_handle, window, |this, _window, cx| {
                if let Some(lhs) = &mut this.lhs {
                    if lhs.was_last_focused {
                        lhs.was_last_focused = false;
                        cx.notify();
                    }
                }
            }),
        );

        let rhs_display_map = self.rhs_editor.read(cx).display_map.clone();
        let lhs_display_map = lhs_editor.read(cx).display_map.clone();
        let rhs_display_map_id = rhs_display_map.entity_id();
        let companion = cx.new(|_| Companion::new(rhs_display_map_id));
        let lhs = LhsEditor {
            editor: lhs_editor,
            multibuffer: lhs_multibuffer,
            was_last_focused: false,
            _subscriptions: subscriptions,
        };

        self.rhs_editor.update(cx, |editor, cx| {
            editor.set_delegate_expand_excerpts(true);
            editor.buffer().update(cx, |rhs_multibuffer, cx| {
                rhs_multibuffer.set_show_deleted_hunks(false, cx);
                rhs_multibuffer.set_use_extended_diff_range(true, cx);
            })
        });

        self.lhs = Some(lhs);

        self.sync_lhs_for_paths(all_paths, cx);

        rhs_display_map.update(cx, |dm, cx| {
            dm.set_companion(Some((lhs_display_map, companion.clone())), cx);
        });

        let lhs = self.lhs.as_ref().unwrap();

        let shared_scroll_anchor = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .scroll_anchor_entity();
        lhs.editor.update(cx, |editor, _cx| {
            editor
                .scroll_manager
                .set_shared_scroll_anchor(shared_scroll_anchor);
        });

        let this = cx.entity().downgrade();
        self.rhs_editor.update(cx, |editor, _cx| {
            let this = this.clone();
            editor.set_on_local_selections_changed(Some(Box::new(
                move |cursor_position, window, cx| {
                    let this = this.clone();
                    window.defer(cx, move |window, cx| {
                        this.update(cx, |this, cx| {
                            this.sync_cursor_to_other_side(true, cursor_position, window, cx);
                        })
                        .ok();
                    })
                },
            )));
        });
        lhs.editor.update(cx, |editor, _cx| {
            let this = this.clone();
            editor.set_on_local_selections_changed(Some(Box::new(
                move |cursor_position, window, cx| {
                    let this = this.clone();
                    window.defer(cx, move |window, cx| {
                        this.update(cx, |this, cx| {
                            this.sync_cursor_to_other_side(false, cursor_position, window, cx);
                        })
                        .ok();
                    })
                },
            )));
        });

        // Copy soft wrap state from rhs (source of truth) to lhs
        let rhs_soft_wrap_override = self.rhs_editor.read(cx).soft_wrap_mode_override;
        lhs.editor.update(cx, |editor, cx| {
            editor.soft_wrap_mode_override = rhs_soft_wrap_override;
            cx.notify();
        });

        cx.notify();
    }

    fn diff_paths(&self, cx: &App) -> Vec<(PathKey, Entity<BufferDiff>)> {
        let rhs_multibuffer = self.rhs_multibuffer.read(cx);
        let rhs_multibuffer_snapshot = rhs_multibuffer.snapshot(cx);
        rhs_multibuffer_snapshot
            .buffers_with_paths()
            .filter_map(|(buffer, path)| {
                let diff = rhs_multibuffer.diff_for(buffer.remote_id())?;
                Some((path.clone(), diff))
            })
            .collect()
    }

    fn activate_pane_left(
        &mut self,
        _: &ActivatePaneLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(lhs) = &self.lhs {
            if !lhs.was_last_focused {
                lhs.editor.read(cx).focus_handle(cx).focus(window, cx);
                lhs.editor.update(cx, |editor, cx| {
                    editor.request_autoscroll(Autoscroll::fit(), cx);
                });
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn activate_pane_right(
        &mut self,
        _: &ActivatePaneRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                self.rhs_editor.read(cx).focus_handle(cx).focus(window, cx);
                self.rhs_editor.update(cx, |editor, cx| {
                    editor.request_autoscroll(Autoscroll::fit(), cx);
                });
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn sync_cursor_to_other_side(
        &mut self,
        from_rhs: bool,
        source_point: Point,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(lhs) = &self.lhs else {
            return;
        };

        let (source_editor, target_editor) = if from_rhs {
            (&self.rhs_editor, &lhs.editor)
        } else {
            (&lhs.editor, &self.rhs_editor)
        };

        let source_snapshot = source_editor.update(cx, |editor, cx| editor.snapshot(window, cx));
        let target_snapshot = target_editor.update(cx, |editor, cx| editor.snapshot(window, cx));

        let display_point = source_snapshot
            .display_snapshot
            .point_to_display_point(source_point, Bias::Right);
        let display_point = target_snapshot.clip_point(display_point, Bias::Right);
        let target_point = target_snapshot.display_point_to_point(display_point, Bias::Right);

        target_editor.update(cx, |editor, cx| {
            editor.set_suppress_selection_callback(true);
            editor.change_selections(crate::SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([target_point..target_point]);
            });
            editor.set_suppress_selection_callback(false);
        });
    }

    pub fn toggle_split(
        &mut self,
        _: &ToggleSplitDiff,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.diff_view_style {
            DiffViewStyle::Unified => {
                self.diff_view_style = DiffViewStyle::Split;
                if !self.too_narrow_for_split {
                    self.split(window, cx);
                }
            }
            DiffViewStyle::Split => {
                self.diff_view_style = DiffViewStyle::Unified;
                if self.is_split() {
                    self.unsplit(window, cx);
                }
            }
        }
    }

    fn intercept_toggle_breakpoint(
        &mut self,
        _: &ToggleBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn intercept_enable_breakpoint(
        &mut self,
        _: &EnableBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn intercept_disable_breakpoint(
        &mut self,
        _: &DisableBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn intercept_edit_log_breakpoint(
        &mut self,
        _: &EditLogBreakpoint,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only block breakpoint actions when the left (lhs) editor has focus
        if let Some(lhs) = &self.lhs {
            if lhs.was_last_focused {
                cx.stop_propagation();
            } else {
                cx.propagate();
            }
        } else {
            cx.propagate();
        }
    }

    fn intercept_inline_assist(
        &mut self,
        _: &InlineAssist,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lhs.is_some() {
            cx.stop_propagation();
        } else {
            cx.propagate();
        }
    }

    fn toggle_soft_wrap(
        &mut self,
        _: &ToggleSoftWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(lhs) = &self.lhs {
            cx.stop_propagation();

            let is_lhs_focused = lhs.was_last_focused;
            let (focused_editor, other_editor) = if is_lhs_focused {
                (&lhs.editor, &self.rhs_editor)
            } else {
                (&self.rhs_editor, &lhs.editor)
            };

            // Toggle the focused editor
            focused_editor.update(cx, |editor, cx| {
                editor.toggle_soft_wrap(&ToggleSoftWrap, window, cx);
            });

            // Copy the soft wrap state from the focused editor to the other editor
            let soft_wrap_override = focused_editor.read(cx).soft_wrap_mode_override;
            other_editor.update(cx, |editor, cx| {
                editor.soft_wrap_mode_override = soft_wrap_override;
                cx.notify();
            });
        } else {
            cx.propagate();
        }
    }

    fn unsplit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(lhs) = self.lhs.take() else {
            return;
        };

        // Detach the stale lhs editor from the shared scroll anchor while the split companion still exists,
        // so its anchor can be converted to lhs native before rhs tears down split specific state.
        lhs.editor.update(cx, |editor, cx| {
            let lhs_snapshot = editor.display_map.update(cx, |dm, cx| dm.snapshot(cx));
            editor
                .scroll_manager
                .unshare_scroll_anchor(&lhs_snapshot, cx);
            editor.set_on_local_selections_changed(None);
        });

        self.rhs_editor.update(cx, |rhs, cx| {
            let rhs_snapshot = rhs.display_map.update(cx, |dm, cx| dm.snapshot(cx));
            let native_anchor = rhs.scroll_manager.native_anchor(&rhs_snapshot, cx);
            let rhs_display_map_id = rhs_snapshot.display_map_id;
            rhs.scroll_manager
                .scroll_anchor_entity()
                .update(cx, |shared, _| {
                    shared.scroll_anchor = native_anchor;
                    shared.display_map_id = Some(rhs_display_map_id);
                });

            rhs.set_on_local_selections_changed(None);
            rhs.set_delegate_expand_excerpts(false);
            rhs.buffer().update(cx, |buffer, cx| {
                buffer.set_show_deleted_hunks(true, cx);
                buffer.set_use_extended_diff_range(false, cx);
            });
            rhs.display_map.update(cx, |dm, cx| {
                dm.set_companion(None, cx);
            });
        });
        cx.notify();
    }

    pub fn update_excerpts_for_path(
        &mut self,
        path: PathKey,
        buffer: Entity<Buffer>,
        ranges: impl IntoIterator<Item = Range<Point>> + Clone,
        context_line_count: u32,
        diff: Entity<BufferDiff>,
        cx: &mut Context<Self>,
    ) -> bool {
        let has_ranges = ranges.clone().into_iter().next().is_some();
        if self.lhs.is_none() {
            return self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
                let added_a_new_excerpt = rhs_multibuffer.update_excerpts_for_path(
                    path,
                    buffer.clone(),
                    ranges,
                    context_line_count,
                    cx,
                );
                if has_ranges
                    && rhs_multibuffer
                        .diff_for(buffer.read(cx).remote_id())
                        .is_none_or(|old_diff| old_diff.entity_id() != diff.entity_id())
                {
                    rhs_multibuffer.add_diff(diff, cx);
                }
                added_a_new_excerpt
            });
        }

        let result = self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            let added_a_new_excerpt = rhs_multibuffer.update_excerpts_for_path(
                path.clone(),
                buffer.clone(),
                ranges,
                context_line_count,
                cx,
            );
            if has_ranges
                && rhs_multibuffer
                    .diff_for(buffer.read(cx).remote_id())
                    .is_none_or(|old_diff| old_diff.entity_id() != diff.entity_id())
            {
                rhs_multibuffer.add_diff(diff.clone(), cx);
            }
            added_a_new_excerpt
        });

        self.sync_lhs_for_paths(vec![(path, diff)], cx);
        result
    }

    fn expand_excerpts(
        &mut self,
        excerpt_anchors: impl Iterator<Item = Anchor> + Clone,
        lines: u32,
        direction: ExpandExcerptDirection,
        cx: &mut Context<Self>,
    ) {
        if self.lhs.is_none() {
            self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
                rhs_multibuffer.expand_excerpts(excerpt_anchors, lines, direction, cx);
            });
            return;
        }

        let paths: Vec<_> = self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            let snapshot = rhs_multibuffer.snapshot(cx);
            let paths = excerpt_anchors
                .clone()
                .filter_map(|anchor| {
                    let (anchor, _) = snapshot.anchor_to_buffer_anchor(anchor)?;
                    let path = snapshot.path_for_buffer(anchor.buffer_id)?;
                    let diff = rhs_multibuffer.diff_for(anchor.buffer_id)?;
                    Some((path.clone(), diff))
                })
                .collect::<HashMap<_, _>>()
                .into_iter()
                .collect();
            rhs_multibuffer.expand_excerpts(excerpt_anchors, lines, direction, cx);
            paths
        });

        self.sync_lhs_for_paths(paths, cx);
    }

    pub fn remove_excerpts_for_path(&mut self, path: PathKey, cx: &mut Context<Self>) {
        self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            rhs_multibuffer.remove_excerpts(path.clone(), cx);
        });

        if let Some(lhs) = &self.lhs {
            lhs.multibuffer.update(cx, |lhs_multibuffer, cx| {
                lhs_multibuffer.remove_excerpts(path, cx);
            });
        }
    }

    fn search_token(&self) -> SearchToken {
        SearchToken::new(self.focused_side() as u64)
    }

    fn editor_for_token(&self, token: SearchToken) -> Option<&Entity<Editor>> {
        if token.value() == SplitSide::Left as u64 {
            return self.lhs.as_ref().map(|lhs| &lhs.editor);
        }
        Some(&self.rhs_editor)
    }

    fn sync_lhs_for_paths(
        &self,
        paths: Vec<(PathKey, Entity<BufferDiff>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(lhs) = &self.lhs else { return };

        self.rhs_multibuffer.update(cx, |rhs_multibuffer, cx| {
            for (path, diff) in paths {
                let main_buffer_id = diff.read(cx).buffer_id;
                let Some(main_buffer) = rhs_multibuffer.buffer(diff.read(cx).buffer_id) else {
                    lhs.multibuffer.update(cx, |lhs_multibuffer, lhs_cx| {
                        lhs_multibuffer.remove_excerpts(path, lhs_cx);
                    });
                    continue;
                };
                let main_buffer_snapshot = main_buffer.read(cx).snapshot();

                let base_text_buffer = diff.read(cx).base_text_buffer().clone();
                let diff_snapshot = diff.read(cx).snapshot(cx);
                let base_text_buffer_snapshot = base_text_buffer.read(cx).snapshot();

                let mut paired_ranges: Vec<(Range<Point>, ExcerptRange<text::Anchor>)> = Vec::new();

                let mut have_excerpt = false;
                let mut did_merge = false;
                let rhs_multibuffer_snapshot = rhs_multibuffer.snapshot(cx);
                for info in rhs_multibuffer_snapshot.excerpts_for_buffer(main_buffer_id) {
                    have_excerpt = true;
                    let rhs_context = info.context.to_point(&main_buffer_snapshot);
                    let lhs_context = buffer_range_to_base_text_range(
                        &rhs_context,
                        &diff_snapshot,
                        &main_buffer_snapshot,
                    );

                    if let Some((prev_lhs_context, prev_rhs_range)) = paired_ranges.last_mut()
                        && prev_lhs_context.end >= lhs_context.start
                    {
                        did_merge = true;
                        prev_lhs_context.end = lhs_context.end;
                        prev_rhs_range.context.end = info.context.end;
                        continue;
                    }

                    paired_ranges.push((lhs_context, info));
                }

                let (lhs_ranges, rhs_ranges): (Vec<_>, Vec<_>) = paired_ranges.into_iter().unzip();
                let lhs_ranges = lhs_ranges
                    .into_iter()
                    .map(|range| {
                        ExcerptRange::new(base_text_buffer_snapshot.anchor_range_outside(range))
                    })
                    .collect::<Vec<_>>();

                lhs.multibuffer.update(cx, |lhs_multibuffer, lhs_cx| {
                    lhs_multibuffer.update_path_excerpts(
                        path.clone(),
                        base_text_buffer,
                        &base_text_buffer_snapshot,
                        &lhs_ranges,
                        lhs_cx,
                    );
                    if have_excerpt
                        && lhs_multibuffer
                            .diff_for(base_text_buffer_snapshot.remote_id())
                            .is_none_or(|old_diff| old_diff.entity_id() != diff.entity_id())
                    {
                        lhs_multibuffer.add_inverted_diff(
                            diff.clone(),
                            main_buffer.clone(),
                            lhs_cx,
                        );
                    }
                });

                if did_merge {
                    rhs_multibuffer.update_path_excerpts(
                        path,
                        main_buffer,
                        &main_buffer_snapshot,
                        &rhs_ranges,
                        cx,
                    );
                }
            }
        });
    }

    fn width_changed(&mut self, width: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        self.last_width = Some(width);

        let min_ems = EditorSettings::get_global(cx).minimum_split_diff_width;

        let style = self.rhs_editor.read(cx).create_style(cx);
        let font_id = window.text_system().resolve_font(&style.text.font());
        let font_size = style.text.font_size.to_pixels(window.rem_size());
        let em_advance = window
            .text_system()
            .em_advance(font_id, font_size)
            .unwrap_or(font_size);
        let min_width = em_advance * min_ems;
        let is_split = self.lhs.is_some();

        self.too_narrow_for_split = min_ems > 0.0 && width < min_width;

        match self.diff_view_style {
            DiffViewStyle::Unified => {}
            DiffViewStyle::Split => {
                if self.too_narrow_for_split && is_split {
                    self.unsplit(window, cx);
                } else if !self.too_narrow_for_split && !is_split {
                    self.split(window, cx);
                }
            }
        }
    }

    pub fn remove_excerpts_for_buffer(
        &mut self,
        buffer_id: BufferId,
        cx: &mut Context<'_, SplittableEditor>,
    ) {
        let snapshot = self.rhs_multibuffer.read(cx).snapshot(cx);
        let Some(path) = snapshot.path_for_buffer(buffer_id) else {
            return;
        };
        self.remove_excerpts_for_path(path.clone(), cx);
    }
}

#[cfg(test)]
impl SplittableEditor {
    fn check_invariants(&self, quiesced: bool, cx: &mut App) {
        use text::Bias;

        use crate::display_map::Block;
        use crate::display_map::DisplayRow;

        let rhs_snapshot = self
            .rhs_editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));

        let Some(lhs) = self.lhs.as_ref() else {
            assert!(
                rhs_snapshot.companion_snapshot().is_none(),
                "rhs display snapshot should not have a companion when unsplit"
            );

            let shared_scroll_anchor = self
                .rhs_editor
                .read(cx)
                .scroll_manager
                .shared_scroll_anchor(cx);
            if let Some(display_map_id) = shared_scroll_anchor.display_map_id {
                assert_eq!(
                    display_map_id, rhs_snapshot.display_map_id,
                    "unsplit editor should not retain a scroll anchor native to a torn-down split companion"
                );
            }

            let _ = self
                .rhs_editor
                .read(cx)
                .scroll_manager
                .native_anchor(&rhs_snapshot, cx);
            return;
        };

        self.debug_print(cx);
        self.check_excerpt_invariants(quiesced, cx);

        let lhs_snapshot = lhs
            .editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));

        let lhs_companion = lhs_snapshot
            .companion_snapshot()
            .expect("lhs display snapshot should have rhs companion while split");
        assert_eq!(
            lhs_companion.display_map_id, rhs_snapshot.display_map_id,
            "lhs display snapshot companion should point to rhs display map"
        );
        assert!(
            lhs_companion.companion_snapshot().is_none(),
            "embedded companion snapshot should not recursively contain another companion"
        );

        let rhs_companion = rhs_snapshot
            .companion_snapshot()
            .expect("rhs display snapshot should have lhs companion while split");
        assert_eq!(
            rhs_companion.display_map_id, lhs_snapshot.display_map_id,
            "rhs display snapshot companion should point to lhs display map"
        );
        assert!(
            rhs_companion.companion_snapshot().is_none(),
            "embedded companion snapshot should not recursively contain another companion"
        );

        let lhs_scroll_anchor_entity_id = lhs
            .editor
            .read(cx)
            .scroll_manager
            .scroll_anchor_entity()
            .entity_id();
        let rhs_scroll_anchor_entity_id = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .scroll_anchor_entity()
            .entity_id();
        assert_eq!(
            lhs_scroll_anchor_entity_id, rhs_scroll_anchor_entity_id,
            "split editors should share a scroll anchor entity"
        );

        let shared_scroll_anchor = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .shared_scroll_anchor(cx);
        if let Some(display_map_id) = shared_scroll_anchor.display_map_id {
            assert!(
                display_map_id == lhs_snapshot.display_map_id
                    || display_map_id == rhs_snapshot.display_map_id,
                "shared scroll anchor should be native to one side of the split"
            );
        }
        let _ = lhs
            .editor
            .read(cx)
            .scroll_manager
            .native_anchor(&lhs_snapshot, cx);
        let _ = self
            .rhs_editor
            .read(cx)
            .scroll_manager
            .native_anchor(&rhs_snapshot, cx);

        if quiesced {
            let lhs_max_row = lhs_snapshot.max_point().row();
            let rhs_max_row = rhs_snapshot.max_point().row();
            assert_eq!(lhs_max_row, rhs_max_row, "mismatch in display row count");

            let lhs_excerpt_block_rows = lhs_snapshot
                .blocks_in_range(DisplayRow(0)..lhs_max_row + 1)
                .filter(|(_, block)| {
                    matches!(
                        block,
                        Block::BufferHeader { .. } | Block::ExcerptBoundary { .. }
                    )
                })
                .map(|(row, _)| row)
                .collect::<Vec<_>>();
            let rhs_excerpt_block_rows = rhs_snapshot
                .blocks_in_range(DisplayRow(0)..rhs_max_row + 1)
                .filter(|(_, block)| {
                    matches!(
                        block,
                        Block::BufferHeader { .. } | Block::ExcerptBoundary { .. }
                    )
                })
                .map(|(row, _)| row)
                .collect::<Vec<_>>();
            assert_eq!(lhs_excerpt_block_rows, rhs_excerpt_block_rows);

            for (lhs_hunk, rhs_hunk) in lhs_snapshot.diff_hunks().zip(rhs_snapshot.diff_hunks()) {
                assert_eq!(
                    lhs_hunk.diff_base_byte_range, rhs_hunk.diff_base_byte_range,
                    "mismatch in hunks"
                );
                assert_eq!(
                    lhs_hunk.status, rhs_hunk.status,
                    "mismatch in hunk statuses"
                );

                let (lhs_point, rhs_point) =
                    if lhs_hunk.row_range.is_empty() || rhs_hunk.row_range.is_empty() {
                        use multi_buffer::ToPoint as _;

                        let lhs_end = Point::new(lhs_hunk.row_range.end.0, 0);
                        let rhs_end = Point::new(rhs_hunk.row_range.end.0, 0);

                        let lhs_excerpt_end = lhs_snapshot
                            .anchor_in_excerpt(lhs_hunk.excerpt_range.context.end)
                            .unwrap()
                            .to_point(&lhs_snapshot);
                        let lhs_exceeds = lhs_end >= lhs_excerpt_end;
                        let rhs_excerpt_end = rhs_snapshot
                            .anchor_in_excerpt(rhs_hunk.excerpt_range.context.end)
                            .unwrap()
                            .to_point(&rhs_snapshot);
                        let rhs_exceeds = rhs_end >= rhs_excerpt_end;
                        if lhs_exceeds != rhs_exceeds {
                            continue;
                        }

                        (lhs_end, rhs_end)
                    } else {
                        (
                            Point::new(lhs_hunk.row_range.start.0, 0),
                            Point::new(rhs_hunk.row_range.start.0, 0),
                        )
                    };
                let lhs_point = lhs_snapshot.point_to_display_point(lhs_point, Bias::Left);
                let rhs_point = rhs_snapshot.point_to_display_point(rhs_point, Bias::Left);
                assert_eq!(
                    lhs_point.row(),
                    rhs_point.row(),
                    "mismatch in hunk position"
                );
            }
        }
    }

    fn debug_print(&self, cx: &mut App) {
        use crate::DisplayRow;
        use crate::display_map::Block;
        use buffer_diff::DiffHunkStatusKind;

        assert!(
            self.lhs.is_some(),
            "debug_print is only useful when lhs editor exists"
        );

        let lhs = self.lhs.as_ref().unwrap();

        // Get terminal width, default to 80 if unavailable
        let terminal_width = std::env::var("COLUMNS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(80);

        // Each side gets half the terminal width minus the separator
        let separator = " │ ";
        let side_width = (terminal_width - separator.len()) / 2;

        // Get display snapshots for both editors
        let lhs_snapshot = lhs.editor.update(cx, |editor, cx| {
            editor.display_map.update(cx, |map, cx| map.snapshot(cx))
        });
        let rhs_snapshot = self.rhs_editor.update(cx, |editor, cx| {
            editor.display_map.update(cx, |map, cx| map.snapshot(cx))
        });

        let lhs_max_row = lhs_snapshot.max_point().row().0;
        let rhs_max_row = rhs_snapshot.max_point().row().0;
        let max_row = lhs_max_row.max(rhs_max_row);

        // Build a map from display row -> block type string
        // Each row of a multi-row block gets an entry with the same block type
        // For spacers, the ID is included in brackets
        fn build_block_map(
            snapshot: &crate::DisplaySnapshot,
            max_row: u32,
        ) -> std::collections::HashMap<u32, String> {
            let mut block_map = std::collections::HashMap::new();
            for (start_row, block) in
                snapshot.blocks_in_range(DisplayRow(0)..DisplayRow(max_row + 1))
            {
                let (block_type, height) = match block {
                    Block::Spacer {
                        id,
                        height,
                        is_below: _,
                    } => (format!("SPACER[{}]", id.0), *height),
                    Block::ExcerptBoundary { height, .. } => {
                        ("EXCERPT_BOUNDARY".to_string(), *height)
                    }
                    Block::BufferHeader { height, .. } => ("BUFFER_HEADER".to_string(), *height),
                    Block::FoldedBuffer { height, .. } => ("FOLDED_BUFFER".to_string(), *height),
                    Block::Custom(custom) => {
                        ("CUSTOM_BLOCK".to_string(), custom.height.unwrap_or(1))
                    }
                };
                for offset in 0..height {
                    block_map.insert(start_row.0 + offset, block_type.clone());
                }
            }
            block_map
        }

        let lhs_blocks = build_block_map(&lhs_snapshot, lhs_max_row);
        let rhs_blocks = build_block_map(&rhs_snapshot, rhs_max_row);

        fn display_width(s: &str) -> usize {
            unicode_width::UnicodeWidthStr::width(s)
        }

        fn truncate_line(line: &str, max_width: usize) -> String {
            let line_width = display_width(line);
            if line_width <= max_width {
                return line.to_string();
            }
            if max_width < 9 {
                let mut result = String::new();
                let mut width = 0;
                for c in line.chars() {
                    let c_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    if width + c_width > max_width {
                        break;
                    }
                    result.push(c);
                    width += c_width;
                }
                return result;
            }
            let ellipsis = "...";
            let target_prefix_width = 3;
            let target_suffix_width = 3;

            let mut prefix = String::new();
            let mut prefix_width = 0;
            for c in line.chars() {
                let c_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                if prefix_width + c_width > target_prefix_width {
                    break;
                }
                prefix.push(c);
                prefix_width += c_width;
            }

            let mut suffix_chars: Vec<char> = Vec::new();
            let mut suffix_width = 0;
            for c in line.chars().rev() {
                let c_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                if suffix_width + c_width > target_suffix_width {
                    break;
                }
                suffix_chars.push(c);
                suffix_width += c_width;
            }
            suffix_chars.reverse();
            let suffix: String = suffix_chars.into_iter().collect();

            format!("{}{}{}", prefix, ellipsis, suffix)
        }

        fn pad_to_width(s: &str, target_width: usize) -> String {
            let current_width = display_width(s);
            if current_width >= target_width {
                s.to_string()
            } else {
                format!("{}{}", s, " ".repeat(target_width - current_width))
            }
        }

        // Helper to format a single row for one side
        // Format: "ln# diff bytes(cumul) text" or block info
        // Line numbers come from buffer_row in RowInfo (1-indexed for display)
        fn format_row(
            row: u32,
            max_row: u32,
            snapshot: &crate::DisplaySnapshot,
            blocks: &std::collections::HashMap<u32, String>,
            row_infos: &[multi_buffer::RowInfo],
            cumulative_bytes: &[usize],
            side_width: usize,
        ) -> String {
            // Get row info if available
            let row_info = row_infos.get(row as usize);

            // Line number prefix (3 chars + space)
            // Use buffer_row from RowInfo, which is None for block rows
            let line_prefix = if row > max_row {
                "    ".to_string()
            } else if let Some(buffer_row) = row_info.and_then(|info| info.buffer_row) {
                format!("{:>3} ", buffer_row + 1) // 1-indexed for display
            } else {
                "    ".to_string() // block rows have no line number
            };
            let content_width = side_width.saturating_sub(line_prefix.len());

            if row > max_row {
                return format!("{}{}", line_prefix, " ".repeat(content_width));
            }

            // Check if this row is a block row
            if let Some(block_type) = blocks.get(&row) {
                let block_str = format!("~~~[{}]~~~", block_type);
                let formatted = format!("{:^width$}", block_str, width = content_width);
                return format!(
                    "{}{}",
                    line_prefix,
                    truncate_line(&formatted, content_width)
                );
            }

            // Get line text
            let line_text = snapshot.line(DisplayRow(row));
            let line_bytes = line_text.len();

            // Diff status marker
            let diff_marker = match row_info.and_then(|info| info.diff_status.as_ref()) {
                Some(status) => match status.kind {
                    DiffHunkStatusKind::Added => "+",
                    DiffHunkStatusKind::Deleted => "-",
                    DiffHunkStatusKind::Modified => "~",
                },
                None => " ",
            };

            // Cumulative bytes
            let cumulative = cumulative_bytes.get(row as usize).copied().unwrap_or(0);

            // Format: "diff bytes(cumul) text" - use 3 digits for bytes, 4 for cumulative
            let info_prefix = format!("{}{:>3}({:>4}) ", diff_marker, line_bytes, cumulative);
            let text_width = content_width.saturating_sub(info_prefix.len());
            let truncated_text = truncate_line(&line_text, text_width);

            let text_part = pad_to_width(&truncated_text, text_width);
            format!("{}{}{}", line_prefix, info_prefix, text_part)
        }

        // Collect row infos for both sides
        let lhs_row_infos: Vec<_> = lhs_snapshot
            .row_infos(DisplayRow(0))
            .take((lhs_max_row + 1) as usize)
            .collect();
        let rhs_row_infos: Vec<_> = rhs_snapshot
            .row_infos(DisplayRow(0))
            .take((rhs_max_row + 1) as usize)
            .collect();

        // Calculate cumulative bytes for each side (only counting non-block rows)
        let mut lhs_cumulative = Vec::with_capacity((lhs_max_row + 1) as usize);
        let mut cumulative = 0usize;
        for row in 0..=lhs_max_row {
            if !lhs_blocks.contains_key(&row) {
                cumulative += lhs_snapshot.line(DisplayRow(row)).len() + 1; // +1 for newline
            }
            lhs_cumulative.push(cumulative);
        }

        let mut rhs_cumulative = Vec::with_capacity((rhs_max_row + 1) as usize);
        cumulative = 0;
        for row in 0..=rhs_max_row {
            if !rhs_blocks.contains_key(&row) {
                cumulative += rhs_snapshot.line(DisplayRow(row)).len() + 1;
            }
            rhs_cumulative.push(cumulative);
        }

        // Print header
        eprintln!();
        eprintln!("{}", "═".repeat(terminal_width));
        let header_left = format!("{:^width$}", "(LHS)", width = side_width);
        let header_right = format!("{:^width$}", "(RHS)", width = side_width);
        eprintln!("{}{}{}", header_left, separator, header_right);
        eprintln!(
            "{:^width$}{}{:^width$}",
            "ln# diff len(cum) text",
            separator,
            "ln# diff len(cum) text",
            width = side_width
        );
        eprintln!("{}", "─".repeat(terminal_width));

        // Print each row
        for row in 0..=max_row {
            let left = format_row(
                row,
                lhs_max_row,
                &lhs_snapshot,
                &lhs_blocks,
                &lhs_row_infos,
                &lhs_cumulative,
                side_width,
            );
            let right = format_row(
                row,
                rhs_max_row,
                &rhs_snapshot,
                &rhs_blocks,
                &rhs_row_infos,
                &rhs_cumulative,
                side_width,
            );
            eprintln!("{}{}{}", left, separator, right);
        }

        eprintln!("{}", "═".repeat(terminal_width));
        eprintln!("Legend: + added, - deleted, ~ modified, ~~~ block/spacer row");
        eprintln!();
    }

    fn check_excerpt_invariants(&self, quiesced: bool, cx: &gpui::App) {
        let lhs = self.lhs.as_ref().expect("should have lhs editor");

        let rhs_snapshot = self.rhs_multibuffer.read(cx).snapshot(cx);
        let rhs_excerpts = rhs_snapshot.excerpts().collect::<Vec<_>>();
        let lhs_snapshot = lhs.multibuffer.read(cx).snapshot(cx);
        let lhs_excerpts = lhs_snapshot.excerpts().collect::<Vec<_>>();
        assert_eq!(lhs_excerpts.len(), rhs_excerpts.len());

        for (lhs_excerpt, rhs_excerpt) in lhs_excerpts.into_iter().zip(rhs_excerpts) {
            assert_eq!(
                lhs_snapshot
                    .path_for_buffer(lhs_excerpt.context.start.buffer_id)
                    .unwrap(),
                rhs_snapshot
                    .path_for_buffer(rhs_excerpt.context.start.buffer_id)
                    .unwrap(),
                "corresponding excerpts should have the same path"
            );
            let diff = self
                .rhs_multibuffer
                .read(cx)
                .diff_for(rhs_excerpt.context.start.buffer_id)
                .expect("missing diff");
            assert_eq!(
                lhs_excerpt.context.start.buffer_id,
                diff.read(cx).base_text(cx).remote_id(),
                "corresponding lhs excerpt should show diff base text"
            );

            if quiesced {
                let diff_snapshot = diff.read(cx).snapshot(cx);
                let lhs_buffer_snapshot = lhs_snapshot
                    .buffer_for_id(lhs_excerpt.context.start.buffer_id)
                    .unwrap();
                let rhs_buffer_snapshot = rhs_snapshot
                    .buffer_for_id(rhs_excerpt.context.start.buffer_id)
                    .unwrap();
                let lhs_range = lhs_excerpt.context.to_point(&lhs_buffer_snapshot);
                let rhs_range = rhs_excerpt.context.to_point(&rhs_buffer_snapshot);
                let expected_lhs_range = buffer_range_to_base_text_range(
                    &rhs_range,
                    &diff_snapshot,
                    &rhs_buffer_snapshot,
                );
                assert_eq!(
                    lhs_range, expected_lhs_range,
                    "corresponding lhs excerpt should have a matching range"
                )
            }
        }
    }
}

impl Item for SplittableEditor {
    type Event = EditorEvent;

    fn tab_content_text(&self, detail: usize, cx: &App) -> ui::SharedString {
        self.rhs_editor.read(cx).tab_content_text(detail, cx)
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<ui::SharedString> {
        self.rhs_editor.read(cx).tab_tooltip_text(cx)
    }

    fn tab_icon(&self, window: &Window, cx: &App) -> Option<ui::Icon> {
        self.rhs_editor.read(cx).tab_icon(window, cx)
    }

    fn tab_content(&self, params: TabContentParams, window: &Window, cx: &App) -> gpui::AnyElement {
        self.rhs_editor.read(cx).tab_content(params, window, cx)
    }

    fn to_item_events(event: &EditorEvent, f: &mut dyn FnMut(ItemEvent)) {
        Editor::to_item_events(event, f)
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(gpui::EntityId, &dyn project::ProjectItem),
    ) {
        self.rhs_editor.read(cx).for_each_project_item(cx, f)
    }

    fn buffer_kind(&self, cx: &App) -> ItemBufferKind {
        self.rhs_editor.read(cx).buffer_kind(cx)
    }

    fn active_project_path(&self, cx: &App) -> Option<project::ProjectPath> {
        self.rhs_editor.read(cx).active_project_path(cx)
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).is_dirty(cx)
    }

    fn has_conflict(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).has_conflict(cx)
    }

    fn has_deleted_file(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).has_deleted_file(cx)
    }

    fn capability(&self, cx: &App) -> language::Capability {
        self.rhs_editor.read(cx).capability(cx)
    }

    fn can_save(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).can_save(cx)
    }

    fn can_save_as(&self, cx: &App) -> bool {
        self.rhs_editor.read(cx).can_save_as(cx)
    }

    fn save(
        &mut self,
        options: SaveOptions,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        self.rhs_editor
            .update(cx, |editor, cx| editor.save(options, project, window, cx))
    }

    fn save_as(
        &mut self,
        project: Entity<Project>,
        path: project::ProjectPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        self.rhs_editor
            .update(cx, |editor, cx| editor.save_as(project, path, window, cx))
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        self.rhs_editor
            .update(cx, |editor, cx| editor.reload(project, window, cx))
    }

    fn navigate(
        &mut self,
        data: Arc<dyn std::any::Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.focused_editor()
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focused_editor().update(cx, |editor, cx| {
            editor.deactivated(window, cx);
        });
    }

    fn added_to_workspace(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace = workspace.weak_handle();
        self.rhs_editor.update(cx, |rhs_editor, cx| {
            rhs_editor.added_to_workspace(workspace, window, cx);
        });
        if let Some(lhs) = &self.lhs {
            lhs.editor.update(cx, |lhs_editor, cx| {
                lhs_editor.added_to_workspace(workspace, window, cx);
            });
        }
    }

    fn as_searchable(
        &self,
        handle: &Entity<Self>,
        _: &App,
    ) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(handle.clone()))
    }

    fn breadcrumb_location(&self, cx: &App) -> ToolbarItemLocation {
        self.rhs_editor.read(cx).breadcrumb_location(cx)
    }

    fn breadcrumbs(&self, cx: &App) -> Option<(Vec<HighlightedText>, Option<Font>)> {
        self.rhs_editor.read(cx).breadcrumbs(cx)
    }

    fn pixel_position_of_cursor(&self, cx: &App) -> Option<gpui::Point<gpui::Pixels>> {
        self.focused_editor().read(cx).pixel_position_of_cursor(cx)
    }

    fn act_as_type<'a>(
        &'a self,
        type_id: std::any::TypeId,
        self_handle: &'a Entity<Self>,
        _: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == std::any::TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == std::any::TypeId::of::<Editor>() {
            Some(self.rhs_editor.clone().into())
        } else {
            None
        }
    }
}

impl SearchableItem for SplittableEditor {
    type Match = Range<Anchor>;

    fn clear_matches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rhs_editor.update(cx, |editor, cx| {
            editor.clear_matches(window, cx);
        });
        if let Some(lhs_editor) = self.lhs_editor() {
            lhs_editor.update(cx, |editor, cx| {
                editor.clear_matches(window, cx);
            })
        }
    }

    fn update_matches(
        &mut self,
        matches: &[Self::Match],
        active_match_index: Option<usize>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.update_matches(matches, active_match_index, token, window, cx);
        });
    }

    fn search_bar_visibility_changed(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if visible {
            let side = self.focused_side();
            self.searched_side = Some(side);
            match side {
                SplitSide::Left => {
                    self.rhs_editor.update(cx, |editor, cx| {
                        editor.clear_matches(window, cx);
                    });
                }
                SplitSide::Right => {
                    if let Some(lhs) = &self.lhs {
                        lhs.editor.update(cx, |editor, cx| {
                            editor.clear_matches(window, cx);
                        });
                    }
                }
            }
        } else {
            self.searched_side = None;
        }
    }

    fn query_suggestion(
        &mut self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> String {
        self.focused_editor().update(cx, |editor, cx| {
            editor.query_suggestion(seed_query_override, window, cx)
        })
    }

    fn activate_match(
        &mut self,
        index: usize,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.activate_match(index, matches, token, window, cx);
        });
    }

    fn select_matches(
        &mut self,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.select_matches(matches, token, window, cx);
        });
    }

    fn replace(
        &mut self,
        identifier: &Self::Match,
        query: &project::search::SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.editor_for_token(token) else {
            return;
        };
        target.update(cx, |editor, cx| {
            editor.replace(identifier, query, token, window, cx);
        });
    }

    fn find_matches(
        &mut self,
        query: Arc<project::search::SearchQuery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Vec<Self::Match>> {
        self.focused_editor()
            .update(cx, |editor, cx| editor.find_matches(query, window, cx))
    }

    fn find_matches_with_token(
        &mut self,
        query: Arc<project::search::SearchQuery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<(Vec<Self::Match>, SearchToken)> {
        let token = self.search_token();
        let editor = self.focused_editor().downgrade();
        cx.spawn_in(window, async move |_, cx| {
            let Some(matches) = editor
                .update_in(cx, |editor, window, cx| {
                    editor.find_matches(query, window, cx)
                })
                .ok()
            else {
                return (Vec::new(), token);
            };
            (matches.await, token)
        })
    }

    fn active_match_index(
        &mut self,
        direction: workspace::searchable::Direction,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        self.editor_for_token(token)?.update(cx, |editor, cx| {
            editor.active_match_index(direction, matches, token, window, cx)
        })
    }
}

impl EventEmitter<EditorEvent> for SplittableEditor {}
impl EventEmitter<SearchEvent> for SplittableEditor {}
impl Focusable for SplittableEditor {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.focused_editor().read(cx).focus_handle(cx)
    }
}

impl Render for SplittableEditor {
    fn render(
        &mut self,
        _window: &mut ui::Window,
        cx: &mut ui::Context<Self>,
    ) -> impl ui::IntoElement {
        let is_split = self.lhs.is_some();
        let inner = if is_split {
            let style = self.rhs_editor.read(cx).create_style(cx);
            SplitEditorView::new(cx.entity(), style, self.split_state.clone()).into_any_element()
        } else {
            self.rhs_editor.clone().into_any_element()
        };

        let this = cx.entity().downgrade();
        let last_width = self.last_width;

        div()
            .id("splittable-editor")
            .on_action(cx.listener(Self::toggle_split))
            .on_action(cx.listener(Self::activate_pane_left))
            .on_action(cx.listener(Self::activate_pane_right))
            .on_action(cx.listener(Self::intercept_toggle_breakpoint))
            .on_action(cx.listener(Self::intercept_enable_breakpoint))
            .on_action(cx.listener(Self::intercept_disable_breakpoint))
            .on_action(cx.listener(Self::intercept_edit_log_breakpoint))
            .on_action(cx.listener(Self::intercept_inline_assist))
            .capture_action(cx.listener(Self::toggle_soft_wrap))
            .size_full()
            .child(inner)
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let width = bounds.size.width;
                        if last_width == Some(width) {
                            return;
                        }
                        window.defer(cx, move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.width_changed(width, window, cx);
                            })
                            .ok();
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests;

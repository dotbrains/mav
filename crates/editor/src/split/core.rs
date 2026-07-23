use super::*;

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

    pub(crate) fn focused_side(&self) -> SplitSide {
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
}

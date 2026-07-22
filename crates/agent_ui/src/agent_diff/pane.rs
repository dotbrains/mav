use super::*;

pub struct AgentDiffPane {
    multibuffer: Entity<MultiBuffer>,
    editor: Entity<SplittableEditor>,
    thread: Entity<AcpThread>,
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    _subscriptions: Vec<Subscription>,
}

impl AgentDiffPane {
    pub fn deploy(
        thread: Entity<AcpThread>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<Self>> {
        workspace.update(cx, |workspace, cx| {
            Self::deploy_in_workspace(thread, workspace, window, cx)
        })
    }

    pub fn deploy_in_workspace(
        thread: Entity<AcpThread>,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let existing_diff = workspace
            .items_of_type::<AgentDiffPane>(cx)
            .find(|diff| diff.read(cx).thread == thread);

        if let Some(existing_diff) = existing_diff {
            workspace.activate_item(&existing_diff, true, true, window, cx);
            existing_diff
        } else {
            let agent_diff = cx
                .new(|cx| AgentDiffPane::new(thread.clone(), workspace.weak_handle(), window, cx));
            workspace.add_item_to_center(Box::new(agent_diff.clone()), window, cx);
            agent_diff
        }
    }

    pub fn new(
        thread: Entity<AcpThread>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

        let project = thread.read(cx).project().clone();
        let editor = cx.new(|cx| {
            let workspace_entity = workspace.upgrade().expect("workspace must exist");
            let diff_display_editor = SplittableEditor::new(
                EditorSettings::get_global(cx).diff_view_style,
                multibuffer.clone(),
                project.clone(),
                workspace_entity,
                window,
                cx,
            );
            diff_display_editor
                .set_render_diff_hunk_controls(diff_hunk_controls(&thread, workspace.clone()), cx);
            diff_display_editor.set_render_diff_hunks_as_unstaged(cx);
            diff_display_editor.update_editors(cx, |editor, _cx| {
                editor.register_addon(AgentDiffAddon);
            });
            diff_display_editor
        });

        let action_log = thread.read(cx).action_log().clone();

        let mut this = Self {
            _subscriptions: vec![
                cx.observe_in(&action_log, window, |this, _action_log, window, cx| {
                    this.update_excerpts(window, cx)
                }),
                cx.subscribe(&thread, |this, _thread, event, cx| {
                    this.handle_acp_thread_event(event, cx)
                }),
            ],
            multibuffer,
            editor,
            thread,
            focus_handle,
            workspace,
        };
        this.update_excerpts(window, cx);
        this
    }

    fn update_excerpts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let changed_buffers = self
            .thread
            .read(cx)
            .action_log()
            .read(cx)
            .changed_buffers(cx);

        // Sort edited files alphabetically for consistency with Git diff view
        let mut sorted_buffers: Vec<_> = changed_buffers.collect();
        sorted_buffers.sort_by(|(buffer_a, _), (buffer_b, _)| {
            let path_a = buffer_a.read(cx).file().map(|f| f.path().clone());
            let path_b = buffer_b.read(cx).file().map(|f| f.path().clone());
            path_a.cmp(&path_b)
        });

        let mut buffers_to_delete = self
            .multibuffer
            .read(cx)
            .snapshot(cx)
            .excerpts()
            .map(|excerpt| excerpt.context.start.buffer_id)
            .collect::<HashSet<_>>();

        for (buffer, diff_handle) in sorted_buffers {
            if buffer.read(cx).file().is_none() {
                continue;
            }

            let path_key = PathKey::for_buffer(&buffer, cx);
            buffers_to_delete.remove(&buffer.read(cx).remote_id());

            let snapshot = buffer.read(cx).snapshot();

            let diff_hunk_ranges = diff_handle
                .read(cx)
                .snapshot(cx)
                .hunks_intersecting_range(
                    language::Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                    &snapshot,
                )
                .map(|diff_hunk| diff_hunk.buffer_range.to_point(&snapshot))
                .collect::<Vec<_>>();

            let was_empty = self.multibuffer.read(cx).is_empty();
            let is_excerpt_newly_added = self.editor.update(cx, |editor, cx| {
                editor.update_excerpts_for_path(
                    path_key.clone(),
                    buffer.clone(),
                    diff_hunk_ranges,
                    multibuffer_context_lines(cx),
                    diff_handle.clone(),
                    cx,
                )
            });

            let rhs_editor = self.editor.read(cx).rhs_editor().clone();
            rhs_editor.update(cx, |editor, cx| {
                if was_empty {
                    let first_hunk = editor
                        .diff_hunks_in_ranges(
                            &[editor::Anchor::Min..editor::Anchor::Max],
                            &self.multibuffer.read(cx).read(cx),
                        )
                        .next();

                    if let Some(first_hunk) = first_hunk {
                        let first_hunk_start = first_hunk.multi_buffer_range.start;
                        editor.change_selections(Default::default(), window, cx, |selections| {
                            selections.select_anchor_ranges([first_hunk_start..first_hunk_start]);
                        })
                    }
                }

                if is_excerpt_newly_added
                    && buffer
                        .read(cx)
                        .file()
                        .is_some_and(|file| file.disk_state().is_deleted())
                {
                    editor.fold_buffer(snapshot.text.remote_id(), cx)
                }
            });
        }

        self.editor.update(cx, |editor, cx| {
            for buffer_id in buffers_to_delete {
                editor.remove_excerpts_for_buffer(buffer_id, cx);
            }
        });

        if self.multibuffer.read(cx).is_empty()
            && self
                .editor
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        {
            self.focus_handle.focus(window, cx);
        } else if self.focus_handle.is_focused(window) && !self.multibuffer.read(cx).is_empty() {
            self.editor.update(cx, |editor, cx| {
                editor.focus_handle(cx).focus(window, cx);
            });
        }
    }

    fn handle_acp_thread_event(&mut self, event: &AcpThreadEvent, cx: &mut Context<Self>) {
        if let AcpThreadEvent::TitleUpdated = event {
            cx.emit(EditorEvent::TitleChanged);
        }
    }

    pub fn move_to_path(&self, path_key: PathKey, window: &mut Window, cx: &mut App) {
        if let Some(position) = self.multibuffer.read(cx).location_for_path(&path_key, cx) {
            let rhs_editor = self.editor.read(cx).rhs_editor().clone();
            rhs_editor.update(cx, |editor, cx| {
                let first_hunk = editor
                    .diff_hunks_in_ranges(
                        &[position..editor::Anchor::Max],
                        &self.multibuffer.read(cx).read(cx),
                    )
                    .next();

                if let Some(first_hunk) = first_hunk {
                    let first_hunk_start = first_hunk.multi_buffer_range.start;
                    editor.change_selections(Default::default(), window, cx, |selections| {
                        selections.select_anchor_ranges([first_hunk_start..first_hunk_start]);
                    })
                }
            });
        }
    }

    fn keep(&mut self, _: &Keep, window: &mut Window, cx: &mut Context<Self>) {
        let rhs_editor = self.editor.read(cx).rhs_editor().clone();
        rhs_editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            keep_edits_in_selection(editor, &snapshot, &self.thread, window, cx);
        });
    }

    fn reject(&mut self, _: &Reject, window: &mut Window, cx: &mut Context<Self>) {
        let rhs_editor = self.editor.read(cx).rhs_editor().clone();
        rhs_editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            reject_edits_in_selection(
                editor,
                &snapshot,
                &self.thread,
                self.workspace.clone(),
                window,
                cx,
            );
        });
    }

    fn reject_all(&mut self, _: &RejectAll, window: &mut Window, cx: &mut Context<Self>) {
        let rhs_editor = self.editor.read(cx).rhs_editor().clone();
        rhs_editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            reject_edits_in_ranges(
                editor,
                &snapshot,
                &self.thread,
                vec![editor::Anchor::Min..editor::Anchor::Max],
                self.workspace.clone(),
                window,
                cx,
            );
        });
    }

    fn keep_all(&mut self, _: &KeepAll, _window: &mut Window, cx: &mut Context<Self>) {
        let telemetry = ActionLogTelemetry::from(self.thread.read(cx));
        let action_log = self.thread.read(cx).action_log().clone();
        action_log.update(cx, |action_log, cx| {
            action_log.keep_all_edits(Some(telemetry), cx)
        });
    }
}

use super::*;

impl FollowableItem for Editor {
    fn remote_id(&self) -> Option<ViewId> {
        self.remote_id
    }

    fn from_state_proto(
        workspace: Entity<Workspace>,
        remote_id: ViewId,
        state: &mut Option<proto::view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Task<Result<Entity<Self>>>> {
        let project = workspace.read(cx).project().to_owned();
        let Some(proto::view::Variant::Editor(_)) = state else {
            return None;
        };
        let Some(proto::view::Variant::Editor(state)) = state.take() else {
            unreachable!()
        };

        let buffer_ids = state
            .path_excerpts
            .iter()
            .map(|excerpt| excerpt.buffer_id)
            .collect::<HashSet<_>>();

        let buffers = project.update(cx, |project, cx| {
            buffer_ids
                .iter()
                .map(|id| BufferId::new(*id).map(|id| project.open_buffer_by_id(id, cx)))
                .collect::<Result<Vec<_>>>()
        });

        Some(window.spawn(cx, async move |cx| {
            let mut buffers = futures::future::try_join_all(buffers?)
                .await
                .debug_assert_ok("leaders don't share views for unshared buffers")?;

            let path_excerpts =
                deserialize_path_excerpts_and_wait_for_anchors(state.path_excerpts, &buffers, cx)
                    .await?;

            let editor = cx.update(|window, cx| {
                let multibuffer = cx.new(|cx| {
                    let mut multibuffer;
                    if state.singleton && buffers.len() == 1 {
                        multibuffer = MultiBuffer::singleton(buffers.pop().unwrap(), cx)
                    } else {
                        multibuffer = MultiBuffer::new(project.read(cx).capability());
                        for (path_key, buffer_id, ranges) in path_excerpts {
                            let Some(buffer) =
                                buffers.iter().find(|b| b.read(cx).remote_id() == buffer_id)
                            else {
                                continue;
                            };
                            let buffer_snapshot = buffer.read(cx).snapshot();
                            multibuffer.update_path_excerpts(
                                path_key,
                                buffer.clone(),
                                &buffer_snapshot,
                                &ranges,
                                cx,
                            );
                        }
                    };

                    if let Some(title) = &state.title {
                        multibuffer = multibuffer.with_title(title.clone())
                    }

                    multibuffer
                });

                cx.new(|cx| {
                    let mut editor =
                        Editor::for_multibuffer(multibuffer, Some(project.clone()), window, cx);
                    editor.remote_id = Some(remote_id);
                    editor
                })
            })?;

            editor.update(cx, |editor, cx| editor.text(cx));
            update_editor_from_message(
                editor.downgrade(),
                project,
                proto::update_view::Editor {
                    selections: state.selections,
                    pending_selection: state.pending_selection,
                    scroll_top_anchor: state.scroll_top_anchor,
                    scroll_x: state.scroll_x,
                    scroll_y: state.scroll_y,
                    ..Default::default()
                },
                cx,
            )
            .await?;

            Ok(editor)
        }))
    }

    fn set_leader_id(
        &mut self,
        leader_id: Option<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leader_id = leader_id;
        if self.leader_id.is_some() {
            self.buffer.update(cx, |buffer, cx| {
                buffer.remove_active_selections(cx);
            });
        } else if self.focus_handle.is_focused(window) {
            self.buffer.update(cx, |buffer, cx| {
                buffer.set_active_selections(
                    &self.selections.disjoint_anchors_arc(),
                    self.selections.line_mode(),
                    self.cursor_shape,
                    cx,
                );
            });
        }
        cx.notify();
    }

    fn to_state_proto(&self, _: &mut Window, cx: &mut App) -> Option<proto::view::Variant> {
        let is_private = self
            .buffer
            .read(cx)
            .as_singleton()
            .and_then(|buffer| buffer.read(cx).file())
            .is_some_and(|file| file.is_private());
        if is_private {
            return None;
        }

        let display_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let scroll_anchor = self.scroll_manager.native_anchor(&display_snapshot, cx);
        let buffer = self.buffer.read(cx);
        let snapshot = buffer.snapshot(cx);
        let mut path_excerpts: Vec<proto::PathExcerpts> = Vec::new();
        for excerpt in snapshot.excerpts() {
            if let Some(prev_entry) = path_excerpts.last_mut()
                && prev_entry.buffer_id == excerpt.context.start.buffer_id.to_proto()
            {
                prev_entry.ranges.push(serialize_excerpt_range(excerpt));
            } else if let Some(path_key) = snapshot.path_for_buffer(excerpt.context.start.buffer_id)
            {
                path_excerpts.push(proto::PathExcerpts {
                    path_key: Some(serialize_path_key(path_key)),
                    buffer_id: excerpt.context.start.buffer_id.to_proto(),
                    ranges: vec![serialize_excerpt_range(excerpt)],
                });
            }
        }

        Some(proto::view::Variant::Editor(proto::view::Editor {
            singleton: buffer.is_singleton(),
            title: buffer.explicit_title().map(ToOwned::to_owned),
            excerpts: Vec::new(),
            scroll_top_anchor: Some(serialize_anchor(&scroll_anchor.anchor)),
            scroll_x: scroll_anchor.offset.x,
            scroll_y: scroll_anchor.offset.y,
            selections: self
                .selections
                .disjoint_anchors_arc()
                .iter()
                .map(serialize_selection)
                .collect(),
            pending_selection: self
                .selections
                .pending_anchor()
                .as_ref()
                .copied()
                .map(serialize_selection),
            path_excerpts,
        }))
    }

    fn to_follow_event(event: &EditorEvent) -> Option<workspace::item::FollowEvent> {
        match event {
            EditorEvent::Edited { .. } => Some(FollowEvent::Unfollow),
            EditorEvent::SelectionsChanged { local }
            | EditorEvent::ScrollPositionChanged { local, .. } => {
                if *local {
                    Some(FollowEvent::Unfollow)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn add_event_to_update_proto(
        &self,
        event: &EditorEvent,
        update: &mut Option<proto::update_view::Variant>,
        _: &mut Window,
        cx: &mut App,
    ) -> bool {
        let update =
            update.get_or_insert_with(|| proto::update_view::Variant::Editor(Default::default()));

        match update {
            proto::update_view::Variant::Editor(update) => match event {
                EditorEvent::BufferRangesUpdated {
                    buffer,
                    path_key,
                    ranges,
                } => {
                    let buffer_id = buffer.read(cx).remote_id().to_proto();
                    let path_key = serialize_path_key(path_key);
                    let ranges = ranges
                        .iter()
                        .cloned()
                        .map(serialize_excerpt_range)
                        .collect::<Vec<_>>();
                    update.updated_paths.push(proto::PathExcerpts {
                        path_key: Some(path_key),
                        buffer_id,
                        ranges,
                    });
                    true
                }
                EditorEvent::BuffersRemoved { removed_buffer_ids } => {
                    update
                        .deleted_buffers
                        .extend(removed_buffer_ids.iter().copied().map(BufferId::to_proto));
                    true
                }
                EditorEvent::ScrollPositionChanged { autoscroll, .. } if !autoscroll => {
                    let display_snapshot = self.display_map.update(cx, |map, cx| map.snapshot(cx));
                    let scroll_anchor = self.scroll_manager.native_anchor(&display_snapshot, cx);
                    update.scroll_top_anchor = Some(serialize_anchor(&scroll_anchor.anchor));
                    update.scroll_x = scroll_anchor.offset.x;
                    update.scroll_y = scroll_anchor.offset.y;
                    true
                }
                EditorEvent::SelectionsChanged { .. } => {
                    update.selections = self
                        .selections
                        .disjoint_anchors_arc()
                        .iter()
                        .map(serialize_selection)
                        .collect();
                    update.pending_selection = self
                        .selections
                        .pending_anchor()
                        .as_ref()
                        .copied()
                        .map(serialize_selection);
                    true
                }
                _ => false,
            },
        }
    }

    fn apply_update_proto(
        &mut self,
        project: &Entity<Project>,
        message: update_view::Variant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let update_view::Variant::Editor(message) = message;
        let project = project.clone();
        cx.spawn_in(window, async move |this, cx| {
            update_editor_from_message(this, project, message, cx).await
        })
    }

    fn is_project_item(&self, _window: &Window, _cx: &App) -> bool {
        true
    }

    fn dedup(&self, existing: &Self, _: &Window, cx: &App) -> Option<Dedup> {
        let self_singleton = self.buffer.read(cx).as_singleton()?;
        let other_singleton = existing.buffer.read(cx).as_singleton()?;
        if self_singleton == other_singleton {
            Some(Dedup::KeepExisting)
        } else {
            None
        }
    }

    fn update_agent_location(
        &mut self,
        location: language::Anchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let buffer = self.buffer.read(cx);
        let buffer = buffer.read(cx);
        let Some(position) = buffer.anchor_in_excerpt(location) else {
            return;
        };
        let selection = Selection {
            id: 0,
            reversed: false,
            start: position,
            end: position,
            goal: SelectionGoal::None,
        };
        drop(buffer);
        self.set_selections_from_remote(vec![selection], None, window, cx);
        self.request_autoscroll_remotely(Autoscroll::focused(), cx);
    }
}

async fn update_editor_from_message(
    this: WeakEntity<Editor>,
    project: Entity<Project>,
    message: proto::update_view::Editor,
    cx: &mut AsyncWindowContext,
) -> Result<()> {
    // Open all of the buffers of which excerpts were added to the editor.
    let inserted_excerpt_buffer_ids = message
        .updated_paths
        .iter()
        .map(|insertion| insertion.buffer_id)
        .collect::<HashSet<_>>();
    let inserted_excerpt_buffers = project.update(cx, |project, cx| {
        inserted_excerpt_buffer_ids
            .into_iter()
            .map(|id| BufferId::new(id).map(|id| project.open_buffer_by_id(id, cx)))
            .collect::<Result<Vec<_>>>()
    })?;
    let inserted_excerpt_buffers = try_join_all(inserted_excerpt_buffers).await?;

    let updated_paths = deserialize_path_excerpts_and_wait_for_anchors(
        message.updated_paths,
        &inserted_excerpt_buffers,
        cx,
    )
    .await?;

    // Update the editor's excerpts.
    let buffer_snapshot = this.update(cx, |editor, cx| {
        editor.buffer.update(cx, |multibuffer, cx| {
            for (path_key, buffer_id, ranges) in updated_paths {
                let Some(buffer) = project.read(cx).buffer_for_id(buffer_id, cx) else {
                    continue;
                };

                let buffer_snapshot = buffer.read(cx).snapshot();
                multibuffer.update_path_excerpts(path_key, buffer, &buffer_snapshot, &ranges, cx);
            }

            for buffer_id in message
                .deleted_buffers
                .into_iter()
                .filter_map(|buffer_id| BufferId::new(buffer_id).ok())
            {
                multibuffer.remove_excerpts_for_buffer(buffer_id, cx);
            }

            multibuffer.snapshot(cx)
        })
    })?;

    // Deserialize the editor state.
    let selections = message
        .selections
        .into_iter()
        .filter_map(|selection| deserialize_selection(selection, &buffer_snapshot))
        .collect::<Vec<_>>();
    let pending_selection = message
        .pending_selection
        .and_then(|selection| deserialize_selection(selection, &buffer_snapshot));
    let scroll_top_anchor = message
        .scroll_top_anchor
        .and_then(|selection| deserialize_anchor(selection, &buffer_snapshot));

    // Wait until the buffer has received all of the operations referenced by
    // the editor's new state.
    this.update(cx, |editor, cx| {
        editor.buffer.update(cx, |buffer, cx| {
            buffer.wait_for_anchors(
                selections
                    .iter()
                    .chain(pending_selection.as_ref())
                    .flat_map(|selection| [selection.start, selection.end])
                    .chain(scroll_top_anchor),
                cx,
            )
        })
    })?
    .await?;

    // Update the editor's state.
    this.update_in(cx, |editor, window, cx| {
        if !selections.is_empty() || pending_selection.is_some() {
            editor.set_selections_from_remote(selections, pending_selection, window, cx);
            editor.request_autoscroll_remotely(Autoscroll::newest(), cx);
        } else if let Some(scroll_top_anchor) = scroll_top_anchor {
            editor.set_scroll_anchor_remote(
                ScrollAnchor {
                    anchor: scroll_top_anchor,
                    offset: point(message.scroll_x, message.scroll_y),
                },
                window,
                cx,
            );
        }
    })?;
    Ok(())
}

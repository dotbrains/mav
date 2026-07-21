use super::*;

impl EditorElement {
    pub(super) fn collect_selection_inputs(
        &self,
        start_anchor: Anchor,
        end_anchor: Anchor,
        snapshot: &EditorSnapshot,
        cx: &mut App,
    ) -> (
        Vec<Selection<Point>>,
        Vec<BufferId>,
        HashMap<BufferId, Anchor>,
    ) {
        self.editor_with_selections(cx)
            .map(|editor| {
                editor.update(cx, |editor, cx| {
                    let is_singleton = editor.buffer_kind(cx) == ItemBufferKind::Singleton;

                    // Singleton buffers only need the newest selection anchor here.
                    let selected_buffer_ids = if is_singleton {
                        Vec::new()
                    } else {
                        let all_selections =
                            editor.selections.all::<Point>(&snapshot.display_snapshot);
                        let mut selected_buffer_ids = Vec::with_capacity(all_selections.len());

                        for selection in all_selections {
                            for buffer_id in snapshot
                                .buffer_snapshot()
                                .buffer_ids_for_range(selection.range())
                            {
                                if selected_buffer_ids.last() != Some(&buffer_id) {
                                    selected_buffer_ids.push(buffer_id);
                                }
                            }
                        }

                        selected_buffer_ids
                    };

                    let mut selections = editor
                        .selections
                        .disjoint_in_range(start_anchor..end_anchor, &snapshot.display_snapshot);
                    selections.extend(editor.selections.pending(&snapshot.display_snapshot));

                    let latest_selection_anchors: HashMap<BufferId, Anchor> = if is_singleton {
                        let head = editor.selections.newest_anchor().head();
                        snapshot
                            .buffer_snapshot()
                            .anchor_to_buffer_anchor(head)
                            .map(|(text_anchor, _)| (text_anchor.buffer_id, head))
                            .into_iter()
                            .collect()
                    } else {
                        let all_anchor_selections =
                            editor.selections.all_anchors(&snapshot.display_snapshot);
                        let mut anchors_by_buffer: HashMap<BufferId, (usize, Anchor)> =
                            HashMap::default();
                        for selection in all_anchor_selections.iter() {
                            let head = selection.head();
                            if let Some((text_anchor, _)) =
                                snapshot.buffer_snapshot().anchor_to_buffer_anchor(head)
                            {
                                anchors_by_buffer
                                    .entry(text_anchor.buffer_id)
                                    .and_modify(|(latest_id, latest_anchor)| {
                                        if selection.id > *latest_id {
                                            *latest_id = selection.id;
                                            *latest_anchor = head;
                                        }
                                    })
                                    .or_insert((selection.id, head));
                            }
                        }
                        anchors_by_buffer
                            .into_iter()
                            .map(|(buffer_id, (_, anchor))| (buffer_id, anchor))
                            .collect()
                    };

                    (selections, selected_buffer_ids, latest_selection_anchors)
                })
            })
            .unwrap_or_else(|| (Vec::new(), Vec::new(), HashMap::default()))
    }
}

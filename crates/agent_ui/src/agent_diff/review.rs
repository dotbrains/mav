use super::*;

fn keep_edits_in_selection(
    editor: &mut Editor,
    buffer_snapshot: &MultiBufferSnapshot,
    thread: &Entity<AcpThread>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let ranges = editor
        .selections
        .disjoint_anchor_ranges()
        .collect::<Vec<_>>();

    keep_edits_in_ranges(editor, buffer_snapshot, thread, ranges, window, cx)
}

fn reject_edits_in_selection(
    editor: &mut Editor,
    buffer_snapshot: &MultiBufferSnapshot,
    thread: &Entity<AcpThread>,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let ranges = editor
        .selections
        .disjoint_anchor_ranges()
        .collect::<Vec<_>>();
    reject_edits_in_ranges(
        editor,
        buffer_snapshot,
        thread,
        ranges,
        workspace,
        window,
        cx,
    )
}

fn keep_edits_in_ranges(
    editor: &mut Editor,
    buffer_snapshot: &MultiBufferSnapshot,
    thread: &Entity<AcpThread>,
    ranges: Vec<Range<editor::Anchor>>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let diff_hunks_in_ranges = editor
        .diff_hunks_in_ranges(&ranges, buffer_snapshot)
        .collect::<Vec<_>>();

    update_editor_selection(editor, buffer_snapshot, &diff_hunks_in_ranges, window, cx);

    let multibuffer = editor.buffer().clone();
    for hunk in &diff_hunks_in_ranges {
        let buffer = multibuffer.read(cx).buffer(hunk.buffer_id);
        if let Some(buffer) = buffer {
            let action_log = thread.read(cx).action_log().clone();
            let telemetry = ActionLogTelemetry::from(thread.read(cx));
            action_log.update(cx, |action_log, cx| {
                action_log.keep_edits_in_range(
                    buffer,
                    hunk.buffer_range.clone(),
                    Some(telemetry),
                    cx,
                )
            });
        }
    }
}

fn reject_edits_in_ranges(
    editor: &mut Editor,
    buffer_snapshot: &MultiBufferSnapshot,
    thread: &Entity<AcpThread>,
    ranges: Vec<Range<editor::Anchor>>,
    workspace: WeakEntity<Workspace>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let diff_hunks_in_ranges = editor
        .diff_hunks_in_ranges(&ranges, buffer_snapshot)
        .collect::<Vec<_>>();

    update_editor_selection(editor, buffer_snapshot, &diff_hunks_in_ranges, window, cx);

    let multibuffer = editor.buffer().clone();

    let mut ranges_by_buffer = HashMap::default();
    for hunk in &diff_hunks_in_ranges {
        let buffer = multibuffer.read(cx).buffer(hunk.buffer_id);
        if let Some(buffer) = buffer {
            ranges_by_buffer
                .entry(buffer.clone())
                .or_insert_with(Vec::new)
                .push(hunk.buffer_range.clone());
        }
    }

    let action_log = thread.read(cx).action_log().clone();
    let telemetry = ActionLogTelemetry::from(thread.read(cx));
    let mut undo_buffers = Vec::new();

    for (buffer, ranges) in ranges_by_buffer {
        action_log
            .update(cx, |action_log, cx| {
                let (task, undo_info) =
                    action_log.reject_edits_in_ranges(buffer, ranges, Some(telemetry.clone()), cx);
                undo_buffers.extend(undo_info);
                task
            })
            .detach_and_log_err(cx);
    }
    if !undo_buffers.is_empty() {
        action_log.update(cx, |action_log, _cx| {
            action_log.set_last_reject_undo(LastRejectUndo {
                buffers: undo_buffers,
            });
        });

        if let Some(workspace) = workspace.upgrade() {
            cx.defer(move |cx| {
                workspace.update(cx, |workspace, cx| {
                    crate::ui::show_undo_reject_toast(workspace, action_log, cx);
                });
            });
        }
    }
}

fn update_editor_selection(
    editor: &mut Editor,
    buffer_snapshot: &MultiBufferSnapshot,
    diff_hunks: &[multi_buffer::MultiBufferDiffHunk],
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    let newest_cursor = editor
        .selections
        .newest::<Point>(&editor.display_snapshot(cx))
        .head();

    if !diff_hunks.iter().any(|hunk| {
        hunk.row_range
            .contains(&multi_buffer::MultiBufferRow(newest_cursor.row))
    }) {
        return;
    }

    let target_hunk = {
        diff_hunks
            .last()
            .and_then(|last_kept_hunk| {
                let last_kept_hunk_end = last_kept_hunk.multi_buffer_range.end;
                editor
                    .diff_hunks_in_ranges(
                        &[last_kept_hunk_end..editor::Anchor::Max],
                        buffer_snapshot,
                    )
                    .nth(1)
            })
            .or_else(|| {
                let first_kept_hunk = diff_hunks.first()?;
                let first_kept_hunk_start = first_kept_hunk.multi_buffer_range.start;
                editor
                    .diff_hunks_in_ranges(
                        &[editor::Anchor::Min..first_kept_hunk_start],
                        buffer_snapshot,
                    )
                    .next()
            })
    };

    if let Some(target_hunk) = target_hunk {
        editor.change_selections(Default::default(), window, cx, |selections| {
            let next_hunk_start = target_hunk.multi_buffer_range.start;
            selections.select_anchor_ranges([next_hunk_start..next_hunk_start]);
        })
    }
}

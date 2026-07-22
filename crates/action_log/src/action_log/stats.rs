use super::*;

#[derive(Default, Debug, Clone, Copy)]
pub struct DiffStats {
    pub lines_added: u32,
    pub lines_removed: u32,
}

impl DiffStats {
    pub fn single_file(buffer: &Buffer, diff: &BufferDiff, cx: &App) -> Self {
        let mut stats = DiffStats::default();
        let diff_snapshot = diff.snapshot(cx);
        let buffer_snapshot = buffer.snapshot();
        let base_text = diff_snapshot.base_text();

        for hunk in diff_snapshot.hunks(&buffer_snapshot) {
            let added_rows = hunk.range.end.row.saturating_sub(hunk.range.start.row);
            stats.lines_added += added_rows;

            let base_start = hunk.diff_base_byte_range.start.to_point(base_text).row;
            let base_end = hunk.diff_base_byte_range.end.to_point(base_text).row;
            let removed_rows = base_end.saturating_sub(base_start);
            stats.lines_removed += removed_rows;
        }

        stats
    }

    pub fn all_files(
        changed_buffers: impl IntoIterator<Item = (Entity<Buffer>, Entity<BufferDiff>)>,
        cx: &App,
    ) -> Self {
        let mut total = DiffStats::default();
        for (buffer, diff) in changed_buffers {
            let stats = DiffStats::single_file(buffer.read(cx), diff.read(cx), cx);
            total.lines_added += stats.lines_added;
            total.lines_removed += stats.lines_removed;
        }
        total
    }
}

#[derive(Clone)]
pub struct ActionLogTelemetry {
    pub agent_telemetry_id: SharedString,
    pub session_id: Arc<str>,
}

pub(super) struct ActionLogMetrics {
    lines_removed: u32,
    lines_added: u32,
    language: Option<SharedString>,
}

impl ActionLogMetrics {
    fn for_buffer(buffer: &Buffer) -> Self {
        Self {
            language: buffer.language().map(|l| l.name().0),
            lines_removed: 0,
            lines_added: 0,
        }
    }

    fn add_edits(&mut self, edits: &[Edit<u32>]) {
        for edit in edits {
            self.add_edit(edit);
        }
    }

    fn add_edit(&mut self, edit: &Edit<u32>) {
        self.lines_added += edit.new_len();
        self.lines_removed += edit.old_len();
    }
}

pub(super) fn telemetry_report_accepted_edits(
    telemetry: &ActionLogTelemetry,
    metrics: ActionLogMetrics,
) {
    telemetry::event!(
        "Agent Edits Accepted",
        agent = telemetry.agent_telemetry_id,
        session = telemetry.session_id,
        language = metrics.language,
        lines_added = metrics.lines_added,
        lines_removed = metrics.lines_removed
    );
}

pub(super) fn telemetry_report_rejected_edits(
    telemetry: &ActionLogTelemetry,
    metrics: ActionLogMetrics,
) {
    telemetry::event!(
        "Agent Edits Rejected",
        agent = telemetry.agent_telemetry_id,
        session = telemetry.session_id,
        language = metrics.language,
        lines_added = metrics.lines_added,
        lines_removed = metrics.lines_removed
    );
}

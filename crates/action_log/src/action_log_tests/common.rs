pub(super) use super::super::*;
pub(super) use buffer_diff::DiffHunkStatusKind;
pub(super) use gpui::TestAppContext;
pub(super) use indoc::indoc;
pub(super) use language::Point;
pub(super) use project::{FakeFs, Fs, Project, RemoveOptions};
pub(super) use rand::prelude::*;
pub(super) use serde_json::json;
pub(super) use settings::SettingsStore;
pub(super) use std::{env, ops::Range, path::PathBuf};
pub(super) use util::{RandomCharIter, path};

pub(super) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

#[derive(Debug, PartialEq)]
struct HunkStatus {
    range: Range<Point>,
    diff_status: DiffHunkStatusKind,
    old_text: String,
}

pub(super) fn unreviewed_hunks(
    action_log: &Entity<ActionLog>,
    cx: &TestAppContext,
) -> Vec<(Entity<Buffer>, Vec<HunkStatus>)> {
    cx.read(|cx| {
        action_log
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, diff)| {
                let snapshot = buffer.read(cx).snapshot();
                (
                    buffer,
                    diff.read(cx)
                        .snapshot(cx)
                        .hunks(&snapshot)
                        .map(|hunk| HunkStatus {
                            diff_status: hunk.status().kind,
                            range: hunk.range,
                            old_text: diff
                                .read(cx)
                                .base_text(cx)
                                .text_for_range(hunk.diff_base_byte_range)
                                .collect(),
                        })
                        .collect(),
                )
            })
            .collect()
    })
}

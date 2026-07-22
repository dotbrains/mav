use super::*;
use git::repository::repo_path;
use gpui::Context;
use language::{Point, Rope};
use project::FakeFs;
use rand::prelude::*;
use serde_json::json;
use settings::SettingsStore;
use std::{cmp, env, ops::Range, path::Path, sync::Mutex};
use text::BufferId;
use unindent::Unindent as _;
use util::{RandomCharIter, path};

mod error_notifications;
mod for_rows;
mod ignores_non_git;
mod random;
mod rows_with_edits;

#[track_caller]
pub(super) fn assert_blame_rows(
    blame: &mut GitBlame,
    buffer_id: BufferId,
    rows: Range<u32>,
    expected: Vec<Option<BlameEntry>>,
    cx: &mut Context<GitBlame>,
) {
    pretty_assertions::assert_eq!(
        blame
            .blame_for_rows(
                &rows
                    .map(|row| RowInfo {
                        buffer_row: Some(row),
                        buffer_id: Some(buffer_id),
                        ..Default::default()
                    })
                    .collect::<Vec<_>>(),
                cx
            )
            .collect::<Vec<_>>(),
        expected
            .into_iter()
            .map(|it| Some((buffer_id, it?)))
            .collect::<Vec<_>>()
    );
}

pub(super) fn init_test(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let settings = SettingsStore::test(cx);
        cx.set_global(settings);

        theme_settings::init(theme::LoadThemes::JustBase, cx);

        crate::init(cx);
    });
}

pub(super) fn gen_blame_entries(max_row: u32, rng: &mut StdRng) -> Vec<BlameEntry> {
    let mut last_row = 0;
    let mut blame_entries = Vec::new();
    for ix in 0..5 {
        if last_row < max_row {
            let row_start = rng.random_range(last_row..max_row);
            let row_end = rng.random_range(row_start + 1..cmp::min(row_start + 3, max_row) + 1);
            blame_entries.push(blame_entry(&ix.to_string(), row_start..row_end));
            last_row = row_end;
        } else {
            break;
        }
    }
    blame_entries
}

pub(super) fn blame_entry(sha: &str, range: Range<u32>) -> BlameEntry {
    BlameEntry {
        sha: sha.parse().unwrap(),
        range,
        original_line_number: 0,
        author: None,
        author_mail: None,
        author_time: None,
        author_tz: None,
        committer_name: None,
        committer_email: None,
        committer_time: None,
        committer_tz: None,
        summary: None,
        previous: None,
        filename: String::new(),
    }
}

use std::collections::BTreeMap;

use zeta_prompt::udiff::apply_diff_to_string;

use crate::{
    jumps::Excerpt,
    patch::{Hunk, Patch, PatchLine},
};

pub(super) fn apply_patch_to_documents(
    patch: &str,
    context: &[Excerpt],
) -> BTreeMap<usize, String> {
    let patch = Patch::parse_unified_diff(patch);
    let mut hunks_by_document: BTreeMap<usize, Vec<Hunk>> = BTreeMap::new();

    for hunk in patch.hunks.into_iter().filter(hunk_has_change) {
        if let Some(document_ix) = find_hunk_document(&hunk, context) {
            hunks_by_document.entry(document_ix).or_default().push(hunk);
        }
    }

    hunks_by_document
        .into_iter()
        .filter_map(|(document_ix, hunks)| {
            let document = context.get(document_ix)?;
            let document_patch = diff_for_document_hunks(document, &hunks);
            let text = apply_diff_to_string(&document_patch, &document.content).ok()?;
            Some((document_ix, text))
        })
        .collect()
}

fn find_hunk_document(hunk: &Hunk, context: &[Excerpt]) -> Option<usize> {
    context
        .iter()
        .enumerate()
        .find_map(|(document_ix, document)| {
            if !path_matches(&hunk.filename, &document.path) {
                return None;
            }

            let document_patch = diff_for_document_hunks(document, std::slice::from_ref(hunk));
            apply_diff_to_string(&document_patch, &document.content)
                .is_ok()
                .then_some(document_ix)
        })
}

fn diff_for_document_hunks(document: &Excerpt, hunks: &[Hunk]) -> String {
    let mut diff = String::new();
    diff.push_str(&format!("--- a/{}\n", document.path));
    diff.push_str(&format!("+++ b/{}\n", document.path));

    for hunk in hunks {
        let old_start = adjust_hunk_start(hunk.old_start, &document.row_range);
        let new_start = adjust_hunk_start(hunk.new_start, &document.row_range);
        let old_count = hunk
            .lines
            .iter()
            .filter(|line| matches!(line, PatchLine::Context(_) | PatchLine::Deletion(_)))
            .count();
        let new_count = hunk
            .lines
            .iter()
            .filter(|line| matches!(line, PatchLine::Context(_) | PatchLine::Addition(_)))
            .count();
        diff.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start, old_count, new_start, new_count
        ));
        for line in &hunk.lines {
            match line {
                PatchLine::Context(text) => {
                    diff.push(' ');
                    diff.push_str(text);
                    diff.push('\n');
                }
                PatchLine::Addition(text) => {
                    diff.push('+');
                    diff.push_str(text);
                    diff.push('\n');
                }
                PatchLine::Deletion(text) => {
                    diff.push('-');
                    diff.push_str(text);
                    diff.push('\n');
                }
                PatchLine::Garbage(text) => {
                    diff.push_str(text);
                    diff.push('\n');
                }
            }
        }
    }

    diff
}

fn adjust_hunk_start(start: isize, row_range: &std::ops::Range<u32>) -> isize {
    let Ok(start_row) = u32::try_from(start.saturating_sub(1)) else {
        return start;
    };

    if row_range.start <= start_row && start_row <= row_range.end {
        start.saturating_sub(row_range.start as isize)
    } else {
        start
    }
}

fn hunk_has_change(hunk: &Hunk) -> bool {
    hunk.lines
        .iter()
        .any(|line| matches!(line, PatchLine::Addition(_) | PatchLine::Deletion(_)))
}

pub(super) fn path_matches(patch_path: &str, document_path: &str) -> bool {
    patch_path == document_path
        || strip_first_path_component(patch_path).is_some_and(|stripped| stripped == document_path)
}

fn strip_first_path_component(path: &str) -> Option<&str> {
    path.split_once('/')
        .map(|(_, rest)| rest)
        .filter(|rest| !rest.is_empty())
}

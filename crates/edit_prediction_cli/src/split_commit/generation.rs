use super::cursor::sample_cursor_position;
use super::human_edits::imitate_human_edits;
use super::patch_split::split_ordered_patch;
use super::service_files::{edit_starts_on_service_file, is_service_file};
use super::*;

pub(super) fn sample_split_point(patch: &Patch, rng: &mut dyn rand::RngCore) -> usize {
    let stats = patch.stats();
    let num_edits = stats.added + stats.removed;
    if num_edits == 0 {
        return 0;
    }

    let mut split = rng.random_range(1..=num_edits);
    for _ in 1..MAX_SPLIT_POINT_SAMPLING_ATTEMPTS {
        if !edit_starts_on_service_file(patch, split) {
            break;
        }
        split = rng.random_range(1..=num_edits);
    }

    split
}

pub(super) fn resolve_split_point_value(split_point: SplitPointValue, num_edits: usize) -> usize {
    match split_point {
        SplitPointValue::Fraction(fraction) => {
            let split = (fraction * num_edits as f64).floor() as usize;
            split.min(num_edits)
        }
        SplitPointValue::Index(index) => index.min(num_edits),
    }
}

#[derive(Debug, Clone)]
pub(super) struct GeneratedSplitCommit {
    pub(super) split: usize,
    pub(super) split_commit: SplitCommit,
    pub(super) cursor: CursorPosition,
    pub(super) cursor_from_human_edit: bool,
}

pub(super) fn generate_split_commit_at_split(
    patch: &Patch,
    split: usize,
    rng: &mut dyn rand::RngCore,
) -> Result<GeneratedSplitCommit> {
    let (prefix, suffix) = split_ordered_patch(patch, split);

    let mut split_commit = SplitCommit {
        source_patch: prefix,
        target_patch: suffix,
    };

    let human_edit_seed = rng.random_range(1..=10000u64);
    let (src_patch, tgt_patch, cursor_opt) = imitate_human_edits(
        &split_commit.source_patch,
        &split_commit.target_patch,
        human_edit_seed,
    );
    split_commit.source_patch = src_patch;
    split_commit.target_patch = tgt_patch;

    let cursor_from_human_edit = cursor_opt.is_some();
    let cursor = match cursor_opt {
        Some(cursor) => cursor,
        None => sample_cursor_position(&split_commit, rng)
            .context("failed to sample cursor position")?,
    };

    Ok(GeneratedSplitCommit {
        split,
        split_commit,
        cursor,
        cursor_from_human_edit,
    })
}

pub(super) fn classify_generated_split_commit(
    generated_split_commit: &GeneratedSplitCommit,
) -> Option<SplitPointKind> {
    let target_patch = Patch::parse_unified_diff(&generated_split_commit.split_commit.target_patch);
    let next_edit = locate_edited_line(&target_patch, 0)?;

    if next_edit.filename != generated_split_commit.cursor.file {
        return Some(SplitPointKind::CrossFile);
    }

    if generated_split_commit.cursor_from_human_edit
        && next_edit.target_line_number == generated_split_commit.cursor.line
    {
        return Some(SplitPointKind::Fim);
    }

    let line_distance = next_edit
        .target_line_number
        .abs_diff(generated_split_commit.cursor.line);
    if line_distance <= SAME_FILE_NEAR_LINE_THRESHOLD {
        Some(SplitPointKind::SameFileNear)
    } else {
        Some(SplitPointKind::SameFileFar)
    }
}

/// Cheap necessary condition for a split to be classifiable as `kind`,
/// computed from the full patch without generating the split.
///
/// The cursor ends up either at the first target edit (or, via
/// `imitate_human_edits`, on its line), or at the last source edit. So the
/// edits adjacent to the split bound what classifications are reachable.
/// Line numbers here are in full-patch coordinates, which can drift slightly
/// from split-patch coordinates, so this is a heuristic pre-filter; the final
/// classification is always verified on the generated split.
fn split_can_match_kind(
    edit_locations: &[EditLocation],
    split: usize,
    kind: SplitPointKind,
) -> bool {
    let (Some(previous_edit), Some(next_edit)) = (
        split.checked_sub(1).and_then(|i| edit_locations.get(i)),
        edit_locations.get(split),
    ) else {
        return false;
    };

    match kind {
        SplitPointKind::Fim => matches!(next_edit.patch_line, PatchLine::Addition(_)),
        SplitPointKind::SameFileNear => true,
        SplitPointKind::SameFileFar => {
            previous_edit.filename == next_edit.filename
                && previous_edit
                    .target_line_number
                    .abs_diff(next_edit.target_line_number)
                    > SAME_FILE_NEAR_LINE_THRESHOLD
        }
        SplitPointKind::CrossFile => previous_edit.filename != next_edit.filename,
    }
}

pub(super) fn sample_split_commit_of_kind(
    patch: &Patch,
    kind: SplitPointKind,
    rng: &mut dyn rand::RngCore,
) -> Result<GeneratedSplitCommit> {
    let edit_locations = edit_locations(patch);
    let num_edits = edit_locations.len();

    let mut candidate_splits: Vec<usize> = (1..num_edits)
        .filter(|&split| {
            !edit_locations
                .get(split)
                .is_some_and(|next_edit| is_service_file(&next_edit.filename))
                && split_can_match_kind(&edit_locations, split, kind)
        })
        .collect();
    candidate_splits.shuffle(rng);

    for split in candidate_splits {
        for _ in 0..MAX_SPLIT_POINT_SAMPLING_ATTEMPTS {
            let Ok(generated_split_commit) = generate_split_commit_at_split(patch, split, rng)
            else {
                continue;
            };

            if classify_generated_split_commit(&generated_split_commit) == Some(kind) {
                return Ok(generated_split_commit);
            }
        }
    }

    Err(NoMatchingSplitPointError { kind }.into())
}

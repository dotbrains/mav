use super::*;

/// Split a patch into (extracted, remainder) based on a set of edit indexes.
/// The first returned patch contains only the chosen edits; the second contains
/// everything else with those edits applied (converted into context).
pub fn extract_edits(patch: &Patch, edit_indexes: &BTreeSet<usize>) -> (Patch, Patch) {
    let mut extracted = patch.clone();
    let mut remainder = patch.clone();

    let stats = patch.stats();
    let num_edits = stats.added + stats.removed;
    let this_edits = edit_indexes.iter().cloned().collect::<Vec<_>>();
    let other_edits = (0..num_edits)
        .filter(|i| !edit_indexes.contains(i))
        .collect();

    remove_edits(&mut extracted, other_edits);
    apply_edits(&mut remainder, this_edits);

    (extracted, remainder)
}

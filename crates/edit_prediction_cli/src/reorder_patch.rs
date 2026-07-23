#![allow(unused)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Reorder selected groups of edits (additions & deletions) into a new patch.
///
/// Intuition:
/// Think of the original patch as a timeline of atomic edit indices (0..N),
/// where one edit is one deleted or inserted line.
/// This function recombines these edits into a new patch which can be thought
/// of as a sequence of patches.
///
/// You provide `edits_order` describing logical chunks (e.g., "write a feature",
/// "refactor", "add tests"). For each group the function:
///  1. Extracts those edits
///  2. Appends them to the output patch
///  3. Removes them from an internal remainder so subsequent original indices
///     still point to the right (yet-to-be-extracted) edits.
///
/// The returned `Patch` contains only the edits you listed, emitted group by
/// group. The leftover remainder is discarded.
///
/// Parameters:
/// * `patch` - Source patch
/// * `edits_order` - Vector of sets of original (0-based) edit indexes
///
/// Returns:
/// * A new `Patch` containing the grouped edits in the requested order.
///
/// Example:
/// ```rust
/// use std::collections::BTreeSet;
/// use reorder_patch::{Patch, reorder_edits};
///
/// // Edits (indexes): 0:-old, 1:+new, 2:-old2, 3:+new2, 4:+added
/// let diff = "\
/// --- a/a.txt
/// +++ b/a.txt
/// @@ -1,3 +1,3 @@
///  one
/// -old
/// +new
///  end
/// @@ -5,3 +5,4 @@
///  tail
/// -old2
/// +new2
/// +added
///  fin
/// ";
/// let patch = Patch::parse_unified_diff(diff);
///
/// // First take the part of the second hunk's edits (2),
/// // then the first hunk (0,1), then the rest of the second hunk (3,4)
/// let order = vec![BTreeSet::from([2]), BTreeSet::from([0, 1]), BTreeSet::from([3, 4])];
/// let reordered = reorder_edits(&patch, order);
/// println!("{}", reordered.to_string());
/// ```
pub fn reorder_edits(patch: &Patch, edits_order: Vec<BTreeSet<usize>>) -> Patch {
    let mut result = Patch {
        header: patch.header.clone(),
        hunks: Vec::new(),
    };

    let mut remainder = patch.clone();

    // Indexes in `edits_order` will shift as we apply edits.
    // This structure maps the original index to the actual index.
    let stats = patch.stats();
    let total_edits = stats.added + stats.removed;
    let mut indexes_map = BTreeMap::from_iter((0..total_edits).map(|i| (i, Some(i))));

    for patch_edits_order in edits_order {
        // Skip duplicated indexes that were already processed
        let patch_edits_order = patch_edits_order
            .into_iter()
            .filter(|&i| indexes_map[&i].is_some()) // skip duplicated indexes
            .collect::<BTreeSet<_>>();

        if patch_edits_order.is_empty() {
            continue;
        }

        let order = patch_edits_order
            .iter()
            .map(|&i| {
                indexes_map[&i].unwrap_or_else(|| panic!("Edit index {i} has been already used. Perhaps your spec contains duplicates"))
            })
            .collect::<BTreeSet<_>>();

        let extracted;
        (extracted, remainder) = extract_edits(&remainder, &order);

        result.hunks.extend(extracted.hunks);

        // Update indexes_map to reflect applied edits. For example:
        //
        // Original_index | Removed?  | Mapped_value
        //       0        | false     | 0
        //       1        | true      | None
        //       2        | true      | None
        //       3        | false     | 1

        for index in patch_edits_order {
            indexes_map.insert(index, None);
            for j in (index + 1)..total_edits {
                if let Some(val) = indexes_map[&j] {
                    indexes_map.insert(j, Some(val - 1));
                }
            }
        }
    }

    result
}

#[path = "reorder_patch/extraction.rs"]
mod extraction;
#[path = "reorder_patch/locations.rs"]
mod locations;
#[path = "reorder_patch/model.rs"]
mod model;
#[path = "reorder_patch/mutation.rs"]
mod mutation;
#[path = "reorder_patch/order_spec.rs"]
mod order_spec;

pub use extraction::extract_edits;
pub use locations::{EditLocation, EditType, edit_locations, locate_edited_line};
pub use model::{DiffStats, Hunk, Patch, PatchLine};
pub use mutation::{apply_edits, remove_edits};
pub use order_spec::parse_order_spec;

#[cfg(test)]
#[path = "reorder_patch/tests.rs"]
mod tests;

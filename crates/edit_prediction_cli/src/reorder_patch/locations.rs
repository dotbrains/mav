use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditLocation {
    pub filename: String,
    pub source_line_number: usize,
    pub target_line_number: usize,
    pub patch_line: PatchLine,
    pub hunk_index: usize,
    pub line_index_within_hunk: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub enum EditType {
    Deletion,
    Insertion,
}

pub fn edit_locations(patch: &Patch) -> Vec<EditLocation> {
    let mut edit_locations = Vec::new();

    for (hunk_index, hunk) in patch.hunks.iter().enumerate() {
        let mut old_line_number = hunk.old_start;
        let mut new_line_number = hunk.new_start;
        for (line_index, line) in hunk.lines.iter().enumerate() {
            if matches!(line, PatchLine::Context(_)) {
                old_line_number += 1;
                new_line_number += 1;
                continue;
            }

            if !matches!(line, PatchLine::Addition(_) | PatchLine::Deletion(_)) {
                continue;
            }

            // old  new
            //  1    1       context
            //  2    2       context
            //  3    3      -deleted
            //  4    3      +insert
            //  4    4       more context
            //
            // old   new
            //  1     1      context
            //  2     2      context
            //  3     3     +inserted
            //  3     4      more context
            //
            // old  new
            //  1    1      -deleted
            //
            // old  new
            //  1    1       context
            //  2    2       context
            //  3    3      -deleted
            //  4    3       more context

            edit_locations.push(EditLocation {
                filename: hunk.filename.clone(),
                source_line_number: old_line_number as usize,
                target_line_number: new_line_number as usize,
                patch_line: line.clone(),
                hunk_index,
                line_index_within_hunk: line_index,
            });

            match line {
                PatchLine::Addition(_) => new_line_number += 1,
                PatchLine::Deletion(_) => old_line_number += 1,
                PatchLine::Context(_) => (),
                _ => (),
            };
        }
    }

    edit_locations
}

pub fn locate_edited_line(patch: &Patch, mut edit_index: isize) -> Option<EditLocation> {
    let mut edit_locations = edit_locations(patch);

    if edit_index < 0 {
        edit_index += edit_locations.len() as isize; // take from end
    }
    (0..edit_locations.len())
        .contains(&(edit_index as usize))
        .then(|| edit_locations.swap_remove(edit_index as usize)) // remove to take ownership
}
//
// Helper function to count old and new lines

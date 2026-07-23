use super::*;

pub fn remove_edits(patch: &mut Patch, edit_indexes: Vec<usize>) {
    let mut current_edit_index: isize = -1;
    let mut new_start_delta_by_file: HashMap<String, isize> = HashMap::new();

    for hunk in &mut patch.hunks {
        if !hunk.is_filename_inherited {
            new_start_delta_by_file.insert(hunk.filename.clone(), 0);
        }
        let delta = new_start_delta_by_file
            .entry(hunk.filename.clone())
            .or_insert(0);
        hunk.new_start += *delta;

        hunk.lines = hunk
            .lines
            .drain(..)
            .filter_map(|line| {
                let is_edit = matches!(line, PatchLine::Addition(_) | PatchLine::Deletion(_));
                if is_edit {
                    current_edit_index += 1;
                    if !edit_indexes.contains(&(current_edit_index as usize)) {
                        return Some(line);
                    }
                }
                match line {
                    PatchLine::Addition(_) => {
                        hunk.new_count -= 1;
                        *delta -= 1;
                        None
                    }
                    PatchLine::Deletion(content) => {
                        hunk.new_count += 1;
                        *delta += 1;
                        Some(PatchLine::Context(content))
                    }
                    _ => Some(line),
                }
            })
            .collect();
    }

    patch.normalize_hunks(3);
    patch.remove_empty_hunks();
}

///
/// Apply specified edits in the patch.
///
/// This generates another patch that looks like selected edits are already made
/// and became part of the context
///
/// See also: `remove_edits()`
///
pub fn apply_edits(patch: &mut Patch, edit_indexes: Vec<usize>) {
    let mut current_edit_index: isize = -1;
    let mut delta_by_file: HashMap<String, isize> = HashMap::new();

    for hunk in &mut patch.hunks {
        if !hunk.is_filename_inherited {
            delta_by_file.insert(hunk.filename.clone(), 0);
        }
        let delta = delta_by_file.entry(hunk.filename.clone()).or_insert(0);
        hunk.old_start += *delta;

        hunk.lines = hunk
            .lines
            .drain(..)
            .filter_map(|line| {
                let is_edit = matches!(line, PatchLine::Addition(_) | PatchLine::Deletion(_));
                if is_edit {
                    current_edit_index += 1;
                    if !edit_indexes.contains(&(current_edit_index as usize)) {
                        return Some(line);
                    }
                }
                match line {
                    PatchLine::Addition(content) => {
                        hunk.old_count += 1;
                        *delta += 1;
                        Some(PatchLine::Context(content))
                    }
                    PatchLine::Deletion(_) => {
                        hunk.old_count -= 1;
                        *delta -= 1;
                        None
                    }
                    _ => Some(line),
                }
            })
            .collect();
    }

    patch.normalize_hunks(3);
    patch.remove_empty_hunks();
}

use super::patch_split::position_weight;
use super::utf8::floor_char_boundary;
use super::*;

pub(super) fn weighted_select(weights: &[u32], seed: u64) -> usize {
    if weights.is_empty() {
        return 0;
    }

    let total_weight: u64 = weights.iter().map(|&w| w as u64).sum();
    if total_weight == 0 {
        // Fallback to uniform selection if all weights are zero
        return seed as usize % weights.len();
    }

    // Use seed to select a value in [0, total_weight)
    let target = seed % total_weight;
    let mut cumulative: u64 = 0;

    for (idx, &weight) in weights.iter().enumerate() {
        cumulative += weight as u64;
        if target < cumulative {
            return idx;
        }
    }

    // Fallback to last index
    weights.len() - 1
}

#[derive(Clone, Copy)]
struct CandidateSplit {
    edit_byte_offset: usize,
    weight: u32,
}

fn push_typed_text_candidates(
    candidates: &mut Vec<CandidateSplit>,
    edit_start_byte_offset: usize,
    final_line: &str,
    final_line_start_byte_offset: usize,
    typed_text: &str,
) {
    for (byte_offset, character) in typed_text.char_indices() {
        let next_byte_offset = byte_offset + character.len_utf8();
        let final_line_candidate_byte_offset = final_line_start_byte_offset + next_byte_offset;
        if final_line[..final_line_candidate_byte_offset]
            .trim()
            .is_empty()
        {
            continue;
        }
        candidates.push(CandidateSplit {
            edit_byte_offset: edit_start_byte_offset + next_byte_offset,
            weight: position_weight(final_line, final_line_candidate_byte_offset),
        });
    }
}

fn push_deleted_text_candidates(
    candidates: &mut Vec<CandidateSplit>,
    edit_start_byte_offset: usize,
    deleted_text: &str,
) {
    for (byte_offset, character) in deleted_text.char_indices() {
        candidates.push(CandidateSplit {
            edit_byte_offset: edit_start_byte_offset + byte_offset + character.len_utf8(),
            weight: 2,
        });
    }
}

fn weighted_select_candidate(candidates: &[CandidateSplit], seed: u64) -> Option<CandidateSplit> {
    if candidates.is_empty() {
        return None;
    }

    let total_weight: u64 = candidates
        .iter()
        .map(|candidate| candidate.weight as u64)
        .sum();
    if total_weight == 0 {
        return Some(candidates[seed as usize % candidates.len()]);
    }

    let target = seed % total_weight;
    let mut cumulative: u64 = 0;

    for candidate in candidates {
        cumulative += candidate.weight as u64;
        if target < cumulative {
            return Some(*candidate);
        }
    }

    candidates.last().copied()
}

/// Calculate similarity ratio between two strings (0-100).
pub(super) fn fuzzy_ratio(s1: &str, s2: &str) -> u32 {
    if s1.is_empty() && s2.is_empty() {
        return 100;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0;
    }

    let diff = TextDiff::from_chars(s1, s2);
    let matching: usize = diff
        .ops()
        .iter()
        .filter_map(|op| {
            if matches!(op.tag(), DiffTag::Equal) {
                Some(op.new_range().len())
            } else {
                None
            }
        })
        .sum();

    let total = s1.len() + s2.len();
    ((2 * matching * 100) / total) as u32
}

/// Imitate human edits by introducing partial line edits.
///
/// This function simulates how a human might incrementally type code,
/// rather than making complete line replacements.
pub fn imitate_human_edits(
    source_patch: &str,
    target_patch: &str,
    seed: u64,
) -> (String, String, Option<CursorPosition>) {
    let no_change = (source_patch.to_string(), target_patch.to_string(), None);

    let src_patch = Patch::parse_unified_diff(source_patch);
    let tgt_patch = Patch::parse_unified_diff(target_patch);

    if tgt_patch.hunks.is_empty() {
        return no_change;
    }

    // Try to locate the first edit in target
    let tgt_edit_loc = match locate_edited_line(&tgt_patch, 0) {
        Some(loc) => loc,
        None => return no_change,
    };

    let tgt_is_addition = matches!(tgt_edit_loc.patch_line, PatchLine::Addition(_));
    if !tgt_is_addition {
        return no_change;
    }

    let tgt_line = match &tgt_edit_loc.patch_line {
        PatchLine::Addition(s) => s.clone(),
        _ => return no_change,
    };

    let source_edit_locations = edit_locations(&src_patch);
    let src_edit_loc = source_edit_locations.last().cloned();

    let src_has_edit_at_target_line = source_edit_locations.iter().any(|loc| {
        loc.filename == tgt_edit_loc.filename
            && loc.target_line_number == tgt_edit_loc.target_line_number
    });

    // Check if this is a replacement (deletion followed by insertion on the same line)
    // or a pure insertion (no corresponding deletion in source)
    let is_replacement = src_edit_loc.as_ref().map_or(false, |loc| {
        matches!(loc.patch_line, PatchLine::Deletion(_))
            && loc.filename == tgt_edit_loc.filename
            && loc.target_line_number == tgt_edit_loc.target_line_number
    });

    // If source has an edit at the same line but it's not a replacement (i.e., it's an addition),
    // we shouldn't process this as a pure insertion either
    if !is_replacement && src_has_edit_at_target_line {
        return no_change;
    }

    let src_line = if is_replacement {
        match &src_edit_loc.as_ref().unwrap().patch_line {
            PatchLine::Deletion(s) => s.clone(),
            _ => return no_change,
        }
    } else {
        // Pure insertion: source line is empty
        String::new()
    };

    // Don't process if source and target are the same
    if src_line == tgt_line {
        return no_change;
    }

    // Tokenize both lines
    let src_tokens = tokenize(&src_line);
    let tgt_tokens = tokenize(&tgt_line);

    // Use similar to get diff operations
    let diff = TextDiff::from_slices(&src_tokens, &tgt_tokens);

    let mut candidate_splits = Vec::new();
    let mut edit_byte_offset = 0usize;
    let mut final_line_byte_offset = 0usize;

    for op in diff.ops() {
        match op.tag() {
            DiffTag::Equal => {
                let equal_text: String = op.old_range().map(|i| src_tokens[i]).collect();
                final_line_byte_offset += equal_text.len();
            }
            DiffTag::Replace => {
                let inserted_text: String = op.new_range().map(|i| tgt_tokens[i]).collect();
                let deleted_text: String = op.old_range().map(|i| src_tokens[i]).collect();
                push_typed_text_candidates(
                    &mut candidate_splits,
                    edit_byte_offset,
                    &tgt_line,
                    final_line_byte_offset,
                    &inserted_text,
                );
                push_deleted_text_candidates(
                    &mut candidate_splits,
                    edit_byte_offset + inserted_text.len(),
                    &deleted_text,
                );
                edit_byte_offset += inserted_text.len() + deleted_text.len();
                final_line_byte_offset += inserted_text.len();
            }
            DiffTag::Insert => {
                let inserted_text: String = op.new_range().map(|i| tgt_tokens[i]).collect();
                push_typed_text_candidates(
                    &mut candidate_splits,
                    edit_byte_offset,
                    &tgt_line,
                    final_line_byte_offset,
                    &inserted_text,
                );
                edit_byte_offset += inserted_text.len();
                final_line_byte_offset += inserted_text.len();
            }
            DiffTag::Delete => {
                let deleted_text: String = op.old_range().map(|i| src_tokens[i]).collect();
                push_deleted_text_candidates(
                    &mut candidate_splits,
                    edit_byte_offset,
                    &deleted_text,
                );
                edit_byte_offset += deleted_text.len();
            }
        }
    }

    let Some(selected_split) = weighted_select_candidate(&candidate_splits, seed) else {
        return no_change;
    };
    let split_byte_offset = selected_split.edit_byte_offset;

    let mut edit_index = 0usize;
    let mut new_src = String::new();
    let mut split_found = false;
    let mut last_old_end = 0usize;

    for op in diff.ops() {
        match op.tag() {
            DiffTag::Equal => {
                for i in op.old_range() {
                    new_src.push_str(src_tokens[i]);
                }
                last_old_end = op.old_range().end;
            }
            DiffTag::Replace => {
                // Handle replace as delete + insert
                let del: String = op.old_range().map(|i| src_tokens[i]).collect();
                let ins: String = op.new_range().map(|i| tgt_tokens[i]).collect();
                let repl_len = del.len() + ins.len();
                if edit_index + repl_len >= split_byte_offset {
                    // Split within this replace operation
                    let offset = split_byte_offset - edit_index;
                    if offset < ins.len() {
                        let safe_offset = floor_char_boundary(&ins, offset);
                        new_src.push_str(&ins[..safe_offset]);
                    } else {
                        new_src.push_str(&ins);
                        let del_offset = offset - ins.len();
                        let safe_del_offset = floor_char_boundary(&del, del_offset.min(del.len()));
                        new_src.push_str(&del[..safe_del_offset]);
                    }
                    split_found = true;
                    last_old_end = op.old_range().end;
                    break;
                } else {
                    edit_index += repl_len;
                    new_src.push_str(&ins);
                    last_old_end = op.old_range().end;
                }
            }
            DiffTag::Insert => {
                let repl: String = op.new_range().map(|i| tgt_tokens[i]).collect();
                if edit_index + repl.len() >= split_byte_offset {
                    let offset = split_byte_offset - edit_index;
                    let safe_offset = floor_char_boundary(&repl, offset);
                    new_src.push_str(&repl[..safe_offset]);
                    split_found = true;
                    break;
                } else {
                    edit_index += repl.len();
                    new_src.push_str(&repl);
                }
            }
            DiffTag::Delete => {
                let repl: String = op.old_range().map(|i| src_tokens[i]).collect();
                if edit_index + repl.len() >= split_byte_offset {
                    let offset = split_byte_offset - edit_index;
                    let safe_offset = floor_char_boundary(&repl, offset);
                    new_src.push_str(&repl[..safe_offset]);
                    split_found = true;
                    last_old_end = op.old_range().start + safe_offset.min(op.old_range().len());
                    break;
                } else {
                    edit_index += repl.len();
                    new_src.push_str(&repl);
                    last_old_end = op.old_range().end;
                }
            }
        }
    }

    if !split_found {
        return no_change;
    }

    // Calculate cursor position
    let line = if is_replacement {
        src_edit_loc.as_ref().unwrap().source_line_number
    } else {
        tgt_edit_loc.target_line_number
    };
    let column = new_src.len() + 1;

    // Add remainder of source if similar enough to target remainder
    let remainder_src: String = (last_old_end..src_tokens.len())
        .map(|i| src_tokens[i])
        .collect();
    let remainder_tgt: String = (last_old_end..tgt_tokens.len())
        .filter_map(|i| tgt_tokens.get(i).copied())
        .collect();

    let ratio = fuzzy_ratio(&remainder_src, &remainder_tgt);
    if ratio > 35 {
        new_src.push_str(&remainder_src);
    }

    if new_src.trim().is_empty() {
        return no_change;
    }

    if new_src == src_line {
        return no_change;
    }

    let cursor = CursorPosition {
        file: tgt_edit_loc.filename.clone(),
        line,
        column: column.min(new_src.len()),
        line_length: new_src.len(),
    };

    // Build new source patch with the intermediate line
    let mut new_src_patch = src_patch;
    if is_replacement {
        // For replacements, insert after the deletion line
        let src_loc = src_edit_loc.as_ref().unwrap();
        if let Some(hunk) = new_src_patch.hunks.get_mut(src_loc.hunk_index) {
            hunk.lines.insert(
                src_loc.line_index_within_hunk + 1,
                PatchLine::Addition(new_src.clone()),
            );
            hunk.new_count += 1;
        }
    } else {
        // For pure insertions, insert after the last edit in source patch
        // This imitates human typing - the intermediate content is what the user is currently typing
        let last_src_edit = locate_edited_line(&new_src_patch, -1);

        if let Some(src_loc) = last_src_edit {
            // Insert after the last edit in source
            if let Some(hunk) = new_src_patch.hunks.get_mut(src_loc.hunk_index) {
                hunk.lines.insert(
                    src_loc.line_index_within_hunk + 1,
                    PatchLine::Addition(new_src.clone()),
                );
                hunk.new_count += 1;
            }
        } else {
            // Source patch is empty or has incompatible hunk structure, create a new hunk based on target
            if let Some(tgt_hunk) = tgt_patch.hunks.get(tgt_edit_loc.hunk_index) {
                let mut new_hunk = tgt_hunk.clone();
                // Replace the full addition with the partial one
                new_hunk.lines.clear();
                for (i, line) in tgt_hunk.lines.iter().enumerate() {
                    if i == tgt_edit_loc.line_index_within_hunk {
                        new_hunk.lines.push(PatchLine::Addition(new_src.clone()));
                    } else {
                        match line {
                            PatchLine::Addition(_) => {
                                // Skip other additions from target
                            }
                            _ => new_hunk.lines.push(line.clone()),
                        }
                    }
                }
                new_hunk.new_count = new_hunk.old_count + 1;
                new_src_patch.hunks.push(new_hunk);
                // Copy header from target if source doesn't have one
                if new_src_patch.header.is_empty() {
                    new_src_patch.header = tgt_patch.header.clone();
                }
            }
        }
    }

    // Build new target patch with the intermediate line as deletion
    let mut new_tgt_patch = tgt_patch;
    if let Some(hunk) = new_tgt_patch.hunks.get_mut(tgt_edit_loc.hunk_index) {
        hunk.lines.insert(
            tgt_edit_loc.line_index_within_hunk,
            PatchLine::Deletion(new_src),
        );
        hunk.old_count += 1;
    }

    (
        new_src_patch.to_string(),
        new_tgt_patch.to_string(),
        Some(cursor),
    )
}

use super::*;

pub fn encode_patch_as_output_for_format(
    format: ZetaFormat,
    old_editable_region: &str,
    patch: &str,
    cursor_offset: Option<usize>,
) -> Result<Option<String>> {
    match format {
        ZetaFormat::v0226Hashline => {
            hashline::patch_to_edit_commands(old_editable_region, patch, cursor_offset).map(Some)
        }
        ZetaFormat::V0304VariableEdit => v0304_variable_edit::patch_to_variable_edit_output(
            old_editable_region,
            patch,
            cursor_offset,
        )
        .map(Some),
        ZetaFormat::V0304SeedNoEdits | ZetaFormat::V0306SeedMultiRegions => {
            Ok(seed_coder::no_edits(patch))
        }
        ZetaFormat::V0316SeedMultiRegions => {
            let empty_patch = patch.lines().count() <= 3;
            if empty_patch {
                let marker_offsets = multi_region::compute_marker_offsets(old_editable_region);
                let marker_num =
                    multi_region::nearest_marker_number(cursor_offset, &marker_offsets);
                let tag = multi_region::marker_tag(marker_num);
                Ok(Some(format!(
                    "{tag}{tag}{}",
                    multi_region::V0316_END_MARKER
                )))
            } else {
                Ok(None)
            }
        }
        ZetaFormat::V0318SeedMultiRegions | ZetaFormat::V0420Diagnostics => {
            let empty_patch = patch.lines().count() <= 3;
            if empty_patch {
                let marker_offsets =
                    multi_region::compute_marker_offsets_v0318(old_editable_region);
                let marker_num =
                    multi_region::nearest_marker_number(cursor_offset, &marker_offsets);
                let tag = multi_region::marker_tag(marker_num);
                Ok(Some(format!(
                    "{tag}{tag}{}",
                    multi_region::V0318_END_MARKER
                )))
            } else {
                Ok(None)
            }
        }
        ZetaFormat::V0608QwenMultiRegions => {
            let empty_patch = patch.lines().count() <= 3;
            if empty_patch {
                let marker_offsets =
                    multi_region::compute_marker_offsets_v0318(old_editable_region);
                let marker_num =
                    multi_region::nearest_marker_number(cursor_offset, &marker_offsets);
                let tag = multi_region::marker_tag(marker_num);
                Ok(Some(format!("{tag}{tag}{}", qwen::END_MARKER)))
            } else {
                Ok(None)
            }
        }
        ZetaFormat::V0317SeedMultiRegions => {
            let empty_patch = patch.lines().count() <= 3;
            if empty_patch {
                let tag = multi_region::marker_tag_relative(0);
                Ok(Some(format!(
                    "{tag}{tag}{}",
                    multi_region::V0317_END_MARKER
                )))
            } else {
                Ok(None)
            }
        }
        ZetaFormat::V0327SingleFile => {
            let empty_patch = patch.lines().count() <= 3;
            if empty_patch {
                let marker_offsets =
                    multi_region::compute_marker_offsets_v0318(old_editable_region);
                let marker_num =
                    multi_region::nearest_marker_number(cursor_offset, &marker_offsets);
                let tag = multi_region::marker_tag(marker_num);
                Ok(Some(format!(
                    "{tag}{tag}{}",
                    multi_region::V0327_END_MARKER
                )))
            } else {
                Ok(None)
            }
        }
        ZetaFormat::V0615HashRegions => Ok(None),
        _ => Ok(None),
    }
}

/// Given a `Zeta2PromptInput`, a format, and a patch (with cursor already
/// extracted), produce the expected model output string for training.
pub fn format_expected_output(
    input: &Zeta2PromptInput,
    format: ZetaFormat,
    patch: &str,
    cursor_offset: Option<usize>,
) -> Result<String> {
    if format == ZetaFormat::V0615HashRegions {
        return hashed_regions::encode_patch_as_output(input, patch, cursor_offset, CURSOR_MARKER);
    }

    let (context, editable_range, _, _) = resolve_cursor_region(input, format);
    let mut old_editable = context[editable_range].to_string();
    if !old_editable.is_empty() && !old_editable.ends_with('\n') {
        old_editable.push('\n');
    }

    // Formats with their own output encoding (hashline, variable-edit,
    // multi-region empty patches) are handled here.
    if let Some(output) =
        encode_patch_as_output_for_format(format, &old_editable, patch, cursor_offset)?
    {
        return Ok(output);
    }

    let empty_patch = patch.lines().count() <= 3;

    match format {
        // Multi-region formats: non-empty patches need diff application
        // then marker-span encoding.
        ZetaFormat::V0316SeedMultiRegions => {
            let (new_editable, first_hunk_offset) =
                udiff::apply_diff_to_string_with_hunk_offset(patch, &old_editable)?;
            let cursor_in_new = cursor_in_new_text(cursor_offset, first_hunk_offset, &new_editable);
            multi_region::encode_from_old_and_new_v0316(
                &old_editable,
                &new_editable,
                cursor_in_new,
                CURSOR_MARKER,
                multi_region::V0316_END_MARKER,
            )
        }
        ZetaFormat::V0318SeedMultiRegions | ZetaFormat::V0420Diagnostics => {
            let (new_editable, first_hunk_offset) =
                udiff::apply_diff_to_string_with_hunk_offset(patch, &old_editable)?;
            let cursor_in_new = cursor_in_new_text(cursor_offset, first_hunk_offset, &new_editable);
            multi_region::encode_from_old_and_new_v0318(
                &old_editable,
                &new_editable,
                cursor_in_new,
                CURSOR_MARKER,
                multi_region::V0318_END_MARKER,
            )
        }
        ZetaFormat::V0608QwenMultiRegions => {
            let (new_editable, first_hunk_offset) =
                udiff::apply_diff_to_string_with_hunk_offset(patch, &old_editable)?;
            let cursor_in_new = cursor_in_new_text(cursor_offset, first_hunk_offset, &new_editable);
            multi_region::encode_from_old_and_new_v0318(
                &old_editable,
                &new_editable,
                cursor_in_new,
                CURSOR_MARKER,
                qwen::END_MARKER,
            )
        }
        ZetaFormat::V0327SingleFile => {
            let (new_editable, first_hunk_offset) =
                udiff::apply_diff_to_string_with_hunk_offset(patch, &old_editable)?;
            let cursor_in_new = cursor_in_new_text(cursor_offset, first_hunk_offset, &new_editable);
            multi_region::encode_from_old_and_new_v0318(
                &old_editable,
                &new_editable,
                cursor_in_new,
                CURSOR_MARKER,
                multi_region::V0327_END_MARKER,
            )
        }
        ZetaFormat::V0317SeedMultiRegions => {
            let (new_editable, first_hunk_offset) =
                udiff::apply_diff_to_string_with_hunk_offset(patch, &old_editable)?;
            let cursor_in_new = cursor_in_new_text(cursor_offset, first_hunk_offset, &new_editable);
            multi_region::encode_from_old_and_new_v0317(
                &old_editable,
                &new_editable,
                cursor_in_new,
                CURSOR_MARKER,
                multi_region::V0317_END_MARKER,
            )
        }
        // V0131-style formats and fallback: produce new editable text with
        // cursor marker inserted, followed by the end marker.
        ZetaFormat::V0112MiddleAtEnd
        | ZetaFormat::V0113Ordered
        | ZetaFormat::V0114180EditableRegion
        | ZetaFormat::V0120GitMergeMarkers
        | ZetaFormat::V0131GitMergeMarkersPrefix
        | ZetaFormat::V0211Prefill
        | ZetaFormat::V0211SeedCoder
        | ZetaFormat::v0226Hashline
        | ZetaFormat::V0304VariableEdit
        | ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0615HashRegions => {
            let (mut result, first_hunk_offset) = if empty_patch {
                (old_editable.clone(), None)
            } else {
                udiff::apply_diff_to_string_with_hunk_offset(patch, &old_editable)?
            };

            if let Some(cursor) = cursor_offset {
                let hunk_start = if !empty_patch {
                    first_hunk_offset.unwrap_or(0)
                } else {
                    0
                };
                let offset = (hunk_start + cursor).min(result.len());
                result.insert_str(offset, CURSOR_MARKER);
            }

            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }

            if let Some(end_marker) = output_end_marker_for_format(format) {
                result.push_str(end_marker);
            }

            Ok(result)
        }
    }
}

/// Compute the cursor position within the new text after diff application.
fn cursor_in_new_text(
    cursor_offset: Option<usize>,
    first_hunk_offset: Option<usize>,
    new_text: &str,
) -> Option<usize> {
    cursor_offset.map(|cursor| {
        let hunk_start = first_hunk_offset.unwrap_or(0);
        (hunk_start + cursor).min(new_text.len())
    })
}

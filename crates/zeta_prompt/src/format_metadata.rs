use super::*;

pub fn special_tokens_for_format(format: ZetaFormat) -> &'static [&'static str] {
    match format {
        ZetaFormat::V0112MiddleAtEnd => v0112_middle_at_end::special_tokens(),
        ZetaFormat::V0113Ordered => v0113_ordered::special_tokens(),
        ZetaFormat::V0114180EditableRegion => v0114180_editable_region::special_tokens(),
        ZetaFormat::V0120GitMergeMarkers => v0120_git_merge_markers::special_tokens(),
        ZetaFormat::V0131GitMergeMarkersPrefix => v0131_git_merge_markers_prefix::special_tokens(),
        ZetaFormat::V0211Prefill => v0211_prefill::special_tokens(),
        ZetaFormat::V0211SeedCoder | ZetaFormat::V0331SeedCoderModelPy => {
            seed_coder::special_tokens()
        }
        ZetaFormat::v0226Hashline => hashline::special_tokens(),
        ZetaFormat::V0304VariableEdit => v0304_variable_edit::special_tokens(),
        ZetaFormat::V0304SeedNoEdits => seed_coder::special_tokens(),
        ZetaFormat::V0316SeedMultiRegions => {
            static TOKENS: &[&str] = &[
                seed_coder::FIM_SUFFIX,
                seed_coder::FIM_PREFIX,
                seed_coder::FIM_MIDDLE,
                seed_coder::FILE_MARKER,
                multi_region::V0316_END_MARKER,
                CURSOR_MARKER,
                multi_region::MARKER_TAG_PREFIX,
            ];
            TOKENS
        }
        ZetaFormat::V0318SeedMultiRegions | ZetaFormat::V0420Diagnostics => {
            static TOKENS: &[&str] = &[
                seed_coder::FIM_SUFFIX,
                seed_coder::FIM_PREFIX,
                seed_coder::FIM_MIDDLE,
                seed_coder::FILE_MARKER,
                multi_region::V0318_END_MARKER,
                CURSOR_MARKER,
                multi_region::MARKER_TAG_PREFIX,
            ];
            TOKENS
        }
        ZetaFormat::V0608QwenMultiRegions => {
            static TOKENS: &[&str] = &[
                qwen::FIM_PREFIX,
                qwen::FIM_SUFFIX,
                qwen::FIM_MIDDLE,
                qwen::FILE_MARKER,
                qwen::END_MARKER,
                CURSOR_MARKER,
                multi_region::MARKER_TAG_PREFIX,
            ];
            TOKENS
        }
        ZetaFormat::V0317SeedMultiRegions => {
            static TOKENS: &[&str] = &[
                seed_coder::FIM_SUFFIX,
                seed_coder::FIM_PREFIX,
                seed_coder::FIM_MIDDLE,
                seed_coder::FILE_MARKER,
                multi_region::V0317_END_MARKER,
                CURSOR_MARKER,
                multi_region::RELATIVE_MARKER_TAG_PREFIX,
            ];
            TOKENS
        }
        ZetaFormat::V0615HashRegions => {
            static TOKENS: &[&str] = &[
                seed_coder::FIM_SUFFIX,
                seed_coder::FIM_PREFIX,
                seed_coder::FIM_MIDDLE,
                seed_coder::FILE_MARKER,
                hashed_regions::V0615_END_MARKER,
                CURSOR_MARKER,
                hashed_regions::MARKER_TAG_PREFIX,
            ];
            TOKENS
        }
        ZetaFormat::V0327SingleFile => {
            static TOKENS: &[&str] = &[
                seed_coder::FIM_SUFFIX,
                seed_coder::FIM_PREFIX,
                seed_coder::FIM_MIDDLE,
                seed_coder::FILE_MARKER,
                multi_region::V0327_END_MARKER,
                CURSOR_MARKER,
                multi_region::MARKER_TAG_PREFIX,
            ];
            TOKENS
        }
        ZetaFormat::V0306SeedMultiRegions => {
            static TOKENS: &[&str] = &[
                seed_coder::FIM_SUFFIX,
                seed_coder::FIM_PREFIX,
                seed_coder::FIM_MIDDLE,
                seed_coder::FILE_MARKER,
                seed_coder::START_MARKER,
                seed_coder::SEPARATOR,
                seed_coder::END_MARKER,
                CURSOR_MARKER,
                multi_region::MARKER_TAG_PREFIX,
            ];
            TOKENS
        }
    }
}

/// Returns the (editable_token_limit, context_token_limit) for a given format.
pub fn token_limits_for_format(format: ZetaFormat) -> (usize, usize) {
    match format {
        ZetaFormat::V0112MiddleAtEnd | ZetaFormat::V0113Ordered => (150, 350),
        ZetaFormat::V0114180EditableRegion => (180, 350),
        ZetaFormat::V0120GitMergeMarkers
        | ZetaFormat::V0131GitMergeMarkersPrefix
        | ZetaFormat::V0211Prefill
        | ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::v0226Hashline
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0316SeedMultiRegions
        | ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0420Diagnostics
        | ZetaFormat::V0608QwenMultiRegions
        | ZetaFormat::V0317SeedMultiRegions
        | ZetaFormat::V0327SingleFile
        | ZetaFormat::V0304SeedNoEdits => (350, 150),
        ZetaFormat::V0615HashRegions => (8000, 0),

        ZetaFormat::V0304VariableEdit => (1024, 0),
    }
}

pub fn stop_tokens_for_format(format: ZetaFormat) -> &'static [&'static str] {
    match format {
        ZetaFormat::v0226Hashline => &[hashline::NO_EDITS_COMMAND_MARKER],
        ZetaFormat::V0112MiddleAtEnd
        | ZetaFormat::V0113Ordered
        | ZetaFormat::V0114180EditableRegion
        | ZetaFormat::V0120GitMergeMarkers
        | ZetaFormat::V0131GitMergeMarkersPrefix
        | ZetaFormat::V0211Prefill
        | ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::V0304VariableEdit
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0304SeedNoEdits => &[],
        ZetaFormat::V0316SeedMultiRegions => &[multi_region::V0316_END_MARKER],
        ZetaFormat::V0318SeedMultiRegions | ZetaFormat::V0420Diagnostics => {
            &[multi_region::V0318_END_MARKER]
        }
        ZetaFormat::V0608QwenMultiRegions => &[qwen::END_MARKER],
        ZetaFormat::V0317SeedMultiRegions => &[multi_region::V0317_END_MARKER],
        ZetaFormat::V0327SingleFile => &[multi_region::V0327_END_MARKER],
        ZetaFormat::V0615HashRegions => &[hashed_regions::V0615_END_MARKER],
    }
}

/// Delimiters used by response-only SFT (e.g. Unsloth `train_on_responses_only`)
/// to mask the prompt and train only on the model's completion.
///
/// Both strings must appear verbatim in the prompt produced by
/// [`format_zeta_prompt`] for the same format: `instruction_part` marks the
/// start of an example, and `response_part` is the final marker before the
/// completion begins.
pub struct TrainingDelimiters {
    pub instruction_part: &'static str,
    pub response_part: &'static str,
}

/// Return the response-only training delimiters for a format.
///
/// This match is intentionally exhaustive with no wildcard arm so that adding a
/// new [`ZetaFormat`] fails to compile until its delimiters are specified.
pub fn training_delimiters_for_format(format: ZetaFormat) -> TrainingDelimiters {
    match format {
        ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0316SeedMultiRegions
        | ZetaFormat::V0317SeedMultiRegions
        | ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0327SingleFile
        | ZetaFormat::V0420Diagnostics
        | ZetaFormat::V0615HashRegions => TrainingDelimiters {
            instruction_part: seed_coder::FIM_SUFFIX,
            response_part: seed_coder::FIM_MIDDLE,
        },
        ZetaFormat::V0608QwenMultiRegions => TrainingDelimiters {
            instruction_part: qwen::FIM_PREFIX,
            response_part: qwen::FIM_MIDDLE,
        },
        ZetaFormat::V0112MiddleAtEnd
        | ZetaFormat::V0113Ordered
        | ZetaFormat::V0114180EditableRegion => TrainingDelimiters {
            instruction_part: "<|file_sep|>",
            response_part: "<|fim_middle|>updated\n",
        },
        ZetaFormat::V0120GitMergeMarkers => TrainingDelimiters {
            instruction_part: "<|file_sep|>",
            response_part: v0120_git_merge_markers::SEPARATOR,
        },
        ZetaFormat::V0131GitMergeMarkersPrefix | ZetaFormat::V0211Prefill => TrainingDelimiters {
            instruction_part: "<|file_sep|>",
            response_part: "<|fim_middle|>",
        },
        ZetaFormat::v0226Hashline => TrainingDelimiters {
            instruction_part: "<|file_sep|>",
            response_part: hashline::END_MARKER,
        },
        ZetaFormat::V0304VariableEdit => TrainingDelimiters {
            instruction_part: "<|file_sep|>",
            response_part: "<|fim_prefix|>",
        },
    }
}

/// Return (editable_range, context_range) for the prompt format
pub fn excerpt_ranges_for_format(
    format: ZetaFormat,
    ranges: &ExcerptRanges,
) -> (Range<usize>, Range<usize>) {
    match format {
        ZetaFormat::V0112MiddleAtEnd | ZetaFormat::V0113Ordered => (
            ranges.editable_150.clone(),
            ranges.editable_150_context_350.clone(),
        ),
        ZetaFormat::V0114180EditableRegion => (
            ranges.editable_180.clone(),
            ranges.editable_180_context_350.clone(),
        ),
        ZetaFormat::V0120GitMergeMarkers
        | ZetaFormat::V0131GitMergeMarkersPrefix
        | ZetaFormat::V0211Prefill
        | ZetaFormat::V0211SeedCoder
        | ZetaFormat::V0331SeedCoderModelPy
        | ZetaFormat::v0226Hashline
        | ZetaFormat::V0304SeedNoEdits
        | ZetaFormat::V0306SeedMultiRegions
        | ZetaFormat::V0316SeedMultiRegions
        | ZetaFormat::V0318SeedMultiRegions
        | ZetaFormat::V0317SeedMultiRegions
        | ZetaFormat::V0420Diagnostics
        | ZetaFormat::V0608QwenMultiRegions => (
            ranges.editable_350.clone(),
            ranges.editable_350_context_150.clone(),
        ),
        ZetaFormat::V0327SingleFile => (
            ranges.editable_350_context_150.clone(),
            ranges.context_8192.clone().unwrap_or(
                // shouldn't be used, only for compat with old data/clients
                ranges.editable_350_context_150.clone(),
            ),
        ),
        ZetaFormat::V0615HashRegions => (
            ranges
                .context_8192
                .clone()
                .unwrap_or_else(|| ranges.editable_350_context_150.clone()),
            ranges
                .context_8192
                .clone()
                .unwrap_or_else(|| ranges.editable_350_context_150.clone()),
        ),

        ZetaFormat::V0304VariableEdit => {
            let context = ranges
                .editable_350_context_1024
                .clone()
                .or(ranges.editable_350_context_512.clone())
                .unwrap_or_else(|| ranges.editable_350_context_150.clone());
            (context.clone(), context)
        }
    }
}

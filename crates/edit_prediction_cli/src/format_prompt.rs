use crate::{
    FormatPromptArgs, PredictionProvider,
    example::{ActualCursor, Example, ExamplePrompt},
    headless::EpAppState,
    progress::{ExampleProgress, Step},
    retrieve_context::{ContextRetrievalType, run_context_retrieval},
};
use anyhow::{Context as _, Result, anyhow};
use gpui::AsyncApp;
use std::ops::Range;
use std::sync::Arc;
use zeta_prompt::{
    Zeta2PromptInput, ZetaFormat, format_edit_history_within_budget, format_expected_output,
    format_zeta_prompt,
    hashed_regions::{self, SnippetMarkers},
    max_edit_event_count_for_format, resolve_cursor_region,
};

fn resolved_excerpt_ranges_for_format(
    input: &zeta_prompt::Zeta2PromptInput,
    format: ZetaFormat,
) -> (Range<usize>, Range<usize>) {
    let (_, editable_range_in_context, context_range, _) = resolve_cursor_region(input, format);
    let editable_range = (context_range.start + editable_range_in_context.start)
        ..(context_range.start + editable_range_in_context.end);
    (editable_range, context_range)
}

pub async fn run_format_prompt(
    example: &mut Example,
    args: &FormatPromptArgs,
    app_state: Arc<EpAppState>,
    example_progress: &ExampleProgress,
    cx: AsyncApp,
) -> Result<()> {
    run_context_retrieval(
        example,
        app_state.clone(),
        example_progress,
        vec![ContextRetrievalType::Lsp],
        false,
        cx.clone(),
    )
    .await?;

    // Teacher-jumps addresses every edit through related-file excerpts and
    // hard-errors unless the cursor file is covered by one. Settled-data
    // samples carry a `cursor_excerpt` but were not run through current-file
    // context retrieval, so normalize the input to synthesize a current-file
    // excerpt from it when the cursor isn't already covered (a no-op for proper
    // `ep context --type=current-file` runs).
    if matches!(
        args.provider,
        PredictionProvider::TeacherJumps(_) | PredictionProvider::TeacherJumpsNonBatching(_)
    ) {
        if let Some(prompt_inputs) = example.prompt_inputs.as_mut() {
            hashed_regions::ensure_cursor_file_excerpt(prompt_inputs);
        }
    }

    let step_progress = example_progress.start(Step::FormatPrompt);

    let prompt_inputs = example
        .prompt_inputs
        .as_ref()
        .context("prompt_inputs must be set after context retrieval")?;

    match args.provider {
        PredictionProvider::Teacher(_, zeta_format)
        | PredictionProvider::TeacherNonBatching(_, zeta_format) => {
            step_progress.set_substatus("formatting teacher prompt");

            let (editable_range, context_range) =
                resolved_excerpt_ranges_for_format(prompt_inputs, zeta_format);

            let include_diagnostics = matches!(zeta_format, ZetaFormat::V0420Diagnostics);

            let prompt = TeacherPrompt::format_prompt(
                example,
                editable_range,
                context_range,
                include_diagnostics,
            );
            example.prompt = Some(ExamplePrompt {
                input: prompt,
                expected_output: None,
                rejected_output: None,
                prefill: None,
                provider: args.provider,
            });
        }
        PredictionProvider::TeacherJumps(_) | PredictionProvider::TeacherJumpsNonBatching(_) => {
            step_progress.set_substatus("formatting teacher jumps prompt");

            let prompt = TeacherJumpsPrompt::format_prompt(example, args.related_files_budget)?;
            example.prompt = Some(ExamplePrompt {
                input: prompt,
                expected_output: None,
                rejected_output: None,
                prefill: None,
                provider: args.provider,
            });
        }
        PredictionProvider::Zeta2(zeta_format) => {
            step_progress.set_substatus("formatting zeta2 prompt");

            let prompt = format_zeta_prompt(prompt_inputs, zeta_format);
            let prefill = zeta_prompt::get_prefill(prompt_inputs, zeta_format);
            let expected_output = example
                .spec
                .expected_patches_with_cursor_positions()
                .into_iter()
                .next()
                .and_then(|(expected_patch, expected_cursor_offset)| {
                    format_expected_output(
                        prompt_inputs,
                        zeta_format,
                        &expected_patch,
                        expected_cursor_offset,
                    )
                    .ok()
                });

            let rejected_output = example.spec.rejected_patch.as_ref().and_then(|patch| {
                format_expected_output(prompt_inputs, zeta_format, patch, None).ok()
            });

            example.prompt = prompt.map(|prompt| ExamplePrompt {
                input: prompt,
                expected_output,
                rejected_output,
                provider: args.provider,
                prefill: Some(prefill),
            });
        }
        _ => {
            panic!("Cannot format prompt for {:?}", args.provider);
        }
    };
    Ok(())
}

#[path = "format_prompt/teacher.rs"]
mod teacher;
#[path = "format_prompt/teacher_jumps.rs"]
mod teacher_jumps;
#[path = "format_prompt/utils.rs"]
mod utils;

pub(crate) use teacher::TeacherPrompt;
pub(crate) use teacher_jumps::TeacherJumpsPrompt;
pub(crate) use utils::{
    extract_all_codeblocks, extract_cursor_excerpt_from_example, extract_last_codeblock,
    line_start_offset,
};

#[cfg(test)]
#[path = "format_prompt/tests.rs"]
mod tests;

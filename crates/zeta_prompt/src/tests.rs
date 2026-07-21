use super::*;
use indoc::indoc;

mod fim_formats;
mod parsing;
mod prompt_budget;
mod zeta1;
mod zeta3_hash_regions;

fn make_input(
    cursor_excerpt: &str,
    editable_range: Range<usize>,
    cursor_offset: usize,
    events: Vec<Event>,
    related_files: Vec<RelatedFile>,
) -> Zeta2PromptInput {
    let context_range = 0..cursor_excerpt.len();
    Zeta2PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_excerpt: cursor_excerpt.into(),
        cursor_offset_in_excerpt: cursor_offset,
        excerpt_start_row: None,
        events: events.into_iter().map(Arc::new).collect(),
        related_files: Some(related_files),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges {
            editable_150: editable_range.clone(),
            editable_180: editable_range.clone(),
            editable_350: editable_range,
            editable_150_context_350: context_range.clone(),
            editable_180_context_350: context_range.clone(),
            editable_350_context_150: context_range,
            ..Default::default()
        },
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    }
}

fn make_input_with_context_range(
    excerpt: &str,
    editable_range: Range<usize>,
    context_range: Range<usize>,
    cursor_offset: usize,
) -> Zeta2PromptInput {
    Zeta2PromptInput {
        cursor_path: Path::new("test.rs").into(),
        cursor_excerpt: excerpt.into(),
        cursor_offset_in_excerpt: cursor_offset,
        excerpt_start_row: None,
        events: vec![],
        related_files: Some(vec![]),
        active_buffer_diagnostics: vec![],
        excerpt_ranges: ExcerptRanges {
            editable_150: editable_range.clone(),
            editable_180: editable_range.clone(),
            editable_350: editable_range,
            editable_150_context_350: context_range.clone(),
            editable_180_context_350: context_range.clone(),
            editable_350_context_150: context_range,
            ..Default::default()
        },
        syntax_ranges: None,
        in_open_source_repo: false,
        can_collect_data: false,
        repo_url: None,
    }
}

fn make_event(path: &str, diff: &str) -> Event {
    Event::BufferChange {
        path: Path::new(path).into(),
        old_path: Path::new(path).into(),
        diff: diff.to_string(),
        old_range: 0..diff.len(),
        new_range: 0..diff.len(),
        predicted: false,
        in_open_source_repo: false,
    }
}

fn make_related_file(path: &str, content: &str) -> RelatedFile {
    RelatedFile {
        path: Path::new(path).into(),
        max_row: content.lines().count() as u32,
        excerpts: vec![RelatedExcerpt {
            row_range: 0..content.lines().count() as u32,
            text: content.into(),
            order: 0,
            context_source: ContextSource::Lsp,
        }],
        in_open_source_repo: false,
    }
}

fn format_with_budget(input: &Zeta2PromptInput, max_tokens: usize) -> Option<String> {
    format_prompt_with_budget_for_format(input, ZetaFormat::V0114180EditableRegion, max_tokens)
}

fn budget_with_margin(requested_tokens: usize) -> usize {
    ((requested_tokens as f64) / 0.9).ceil() as usize
}

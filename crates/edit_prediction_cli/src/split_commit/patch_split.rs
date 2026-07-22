use super::cursor::get_cursor_excerpt;
use super::generation::{
    classify_generated_split_commit, generate_split_commit_at_split, resolve_split_point_value,
    sample_split_commit_of_kind, sample_split_point,
};
use super::service_files::has_submodule_gitlink_hunk;
use super::*;

pub fn generate_evaluation_example_from_ordered_commit(
    commit: &str,
    repository_url: &str,
    commit_hash: &str,
    split_point: Option<SplitPoint>,
    seed: Option<u64>,
    sample_num: Option<usize>,
) -> Result<ExampleSpec> {
    anyhow::ensure!(
        !has_submodule_gitlink_hunk(commit),
        "commit contains submodule/gitlink hunk"
    );

    let mut rng: Box<dyn rand::RngCore> = match seed {
        Some(seed) => Box::new(rand::rngs::StdRng::seed_from_u64(seed)),
        None => Box::new(rand::rngs::ThreadRng::default()),
    };

    // Parse and normalize the commit
    let mut patch = Patch::parse_unified_diff(commit);

    // Filter header to only keep lines starting with "//"
    let header_lines: Vec<&str> = patch
        .header
        .lines()
        .filter(|line| line.starts_with("//"))
        .collect();
    patch.header = if header_lines.is_empty() {
        String::new()
    } else {
        header_lines.join("\n") + "\n"
    };

    // Compute the split point
    let stats = patch.stats();
    let num_edits = stats.added + stats.removed;

    anyhow::ensure!(num_edits != 0, "no edits found in commit");

    let generated_split_commit = match split_point {
        None => {
            let split = sample_split_point(&patch, rng.as_mut());
            generate_split_commit_at_split(&patch, split, rng.as_mut())?
        }
        Some(SplitPoint::Fraction(fraction)) => {
            let split = resolve_split_point_value(SplitPointValue::Fraction(fraction), num_edits);
            generate_split_commit_at_split(&patch, split, rng.as_mut())?
        }
        Some(SplitPoint::Index(index)) => {
            let split = resolve_split_point_value(SplitPointValue::Index(index), num_edits);
            generate_split_commit_at_split(&patch, split, rng.as_mut())?
        }
        Some(SplitPoint::Kind(kind)) => sample_split_commit_of_kind(&patch, kind, rng.as_mut())?,
        Some(SplitPoint::KindWithSplit { kind, split_point }) => {
            let split = resolve_split_point_value(split_point, num_edits);
            let generated_split_commit =
                generate_split_commit_at_split(&patch, split, rng.as_mut())?;
            let actual_kind = classify_generated_split_commit(&generated_split_commit);
            anyhow::ensure!(
                actual_kind == Some(kind),
                "split point {split} classified as {}, expected {kind}",
                actual_kind
                    .map(|kind| kind.to_string())
                    .unwrap_or_else(|| "empty-target".to_string())
            );
            generated_split_commit
        }
    };

    let split = generated_split_commit.split;
    let cursor = generated_split_commit.cursor;
    let mut split_commit = generated_split_commit.split_commit;

    // Get cursor excerpt
    let cursor_excerpt = get_cursor_excerpt(
        &cursor,
        &split_commit.source_patch,
        &split_commit.target_patch,
    )
    .context("failed to generate cursor excerpt")?;

    // Where the source patch is empty, there's not enough info to make a
    // meaningful prediction
    if split == 0 {
        split_commit.target_patch = String::new();
    }

    let repo_name = repository_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("unknown");
    let short_sha = &commit_hash[..commit_hash.len().min(8)];
    let name = match sample_num {
        Some(n) => format!("{}-{}-{}", repo_name, short_sha, n),
        None => format!("{}-{}", repo_name, short_sha),
    };

    Ok(ExampleSpec {
        name,
        repository_url: repository_url.to_string(),
        revision: format!("{}~1", commit_hash),
        edit_history: split_commit.source_patch.clone(),
        cursor_path: Path::new(&cursor.file).into(),
        cursor_position: cursor_excerpt,
        expected_patches: vec![split_commit.target_patch],
        tags: vec![],
        reasoning: None,
        uncommitted_diff: String::new(),
        recently_opened_files: Vec::new(),
        recently_viewed_files: Vec::new(),
        uncommitted_diff_contains_edit_history: false,
        rejected_patch: None,

        telemetry: None,
        human_feedback: Vec::new(),
        rating: None,
    })
}

/// Split an ordered commit into source and target commits.
///
/// # Arguments
/// * `commit` - Ordered commit string
/// * `split_pos` - Position to split the commit (number of edited lines)
///
/// # Returns
/// A tuple of (source_diff, target_diff)
pub fn split_ordered_patch(patch: &Patch, split_pos: usize) -> (String, String) {
    let source_edits: BTreeSet<usize> = (0..split_pos).collect();
    let (source, mut target) = extract_edits(patch, &source_edits);
    if !target.hunks.is_empty() {
        if let Some(header) = header_for_edit(patch, split_pos) {
            target.header = header;
        }
    }

    let mut source_str = source.to_string();
    let target_str = target.to_string();

    // Strip last group header from the source (lines starting with "//" at the end)
    let source_lines: Vec<&str> = source_str.lines().collect();
    let mut end_idx = source_lines.len();
    for i in (0..source_lines.len()).rev() {
        if source_lines[i].starts_with("//") {
            end_idx = i;
        } else {
            break;
        }
    }
    if end_idx < source_lines.len() {
        source_str = source_lines[..end_idx].join("\n");
        if !source_str.is_empty() {
            source_str.push('\n');
        }
    }

    (source_str, target_str)
}

fn header_for_edit(patch: &Patch, edit_index: usize) -> Option<String> {
    let edit_index = edit_index.try_into().ok()?;
    let edit_location = locate_edited_line(patch, edit_index)?;
    header_for_hunk(patch, edit_location.hunk_index)
}

fn header_for_hunk(patch: &Patch, hunk_index: usize) -> Option<String> {
    for hunk in patch.hunks.get(..hunk_index)?.iter().rev() {
        let mut header_lines = Vec::new();
        for line in hunk.lines.iter().rev() {
            let PatchLine::Garbage(line) = line else {
                break;
            };
            if line.trim().is_empty() && header_lines.is_empty() {
                continue;
            }
            if !line.starts_with("//") {
                break;
            }
            header_lines.push(line.as_str());
        }
        if !header_lines.is_empty() {
            return Some(render_reversed_header_lines(header_lines));
        }
    }

    let header_lines = patch
        .header
        .lines()
        .rev()
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| line.starts_with("//"))
        .collect::<Vec<_>>();
    (!header_lines.is_empty()).then(|| render_reversed_header_lines(header_lines))
}

fn render_reversed_header_lines(mut lines: Vec<&str>) -> String {
    lines.reverse();
    lines.join("\n") + "\n"
}

/// Calculate the weight for a split byte offset in `text`.
///
/// Higher weights indicate more natural pause points (e.g., after punctuation,
/// at identifier boundaries). Lower weights indicate less natural points
/// (e.g., mid-identifier).
pub(super) fn position_weight(text: &str, byte_offset: usize) -> u32 {
    if byte_offset == 0 || byte_offset > text.len() || !text.is_char_boundary(byte_offset) {
        return 1;
    }

    let Some(prev_char) = text[..byte_offset].chars().next_back() else {
        return 1;
    };
    let next_char = text[byte_offset..].chars().next();

    // High weight: natural pause points (end of statement/argument, opening brackets)
    if matches!(prev_char, ',' | ';' | ':' | '(' | '[' | '{') {
        return 10;
    }

    // High weight: closing brackets (finished a group)
    if matches!(prev_char, ')' | ']' | '}') {
        return 8;
    }

    // Medium weight: operators and method chains
    if matches!(
        prev_char,
        '.' | '+' | '-' | '*' | '/' | '=' | '<' | '>' | '&' | '|' | '!'
    ) {
        return 5;
    }

    // Check if we're at the end of an identifier (word char followed by non-word char)
    let is_prev_word_char = prev_char.is_alphanumeric() || prev_char == '_';
    let is_next_word_char = next_char.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');

    if is_prev_word_char && !is_next_word_char {
        // End of identifier - high weight
        return 8;
    }

    // Whitespace is a natural pause
    if prev_char.is_whitespace() {
        return 6;
    }

    // Mid-identifier: low weight (rare autocomplete scenarios)
    if is_prev_word_char && is_next_word_char {
        return 1;
    }

    // Default medium-low weight
    3
}

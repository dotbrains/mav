use crate::{Grammar, InjectionConfig, Language, LanguageId, LanguageRegistry, with_parser};
use collections::HashMap;
use futures::FutureExt;
use std::{
    borrow::Cow,
    cmp,
    collections::BinaryHeap,
    ops::{ControlFlow, Range},
    sync::Arc,
    time::{Duration, Instant},
};
use streaming_iterator::StreamingIterator;
use text::{Anchor, BufferSnapshot, Point, Rope};
use tree_sitter::Node;

use super::parse::ParseTimeout;
use super::tree::*;
use super::types::*;

#[ztracing::instrument(skip_all)]
pub(super) fn parse_text(
    grammar: &Grammar,
    text: &Rope,
    start_byte: usize,
    ranges: &[tree_sitter::Range],
    old_tree: Option<&tree_sitter::Tree>,
    parse_budget: &mut Option<Duration>,
) -> anyhow::Result<tree_sitter::Tree> {
    with_parser(|parser| {
        let mut timed_out = false;
        let now = Instant::now();
        let mut progress_callback = parse_budget.map(|budget| {
            let timed_out = &mut timed_out;
            move |_: &_| {
                let elapsed = now.elapsed();
                if elapsed > budget {
                    *timed_out = true;
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            }
        });

        let mut chunks = text.chunks_in_range(start_byte..text.len());
        parser.set_included_ranges(ranges)?;
        parser.set_language(&grammar.ts_language)?;
        parser
            .parse_with_options(
                &mut move |offset, _| {
                    chunks.seek(start_byte + offset);
                    chunks.next().unwrap_or("").as_bytes()
                },
                old_tree,
                progress_callback
                    .as_mut()
                    .map(|progress_callback| tree_sitter::ParseOptions {
                        progress_callback: Some(progress_callback),
                    }),
            )
            .inspect(|_| {
                if let Some(parse_budget) = parse_budget {
                    *parse_budget = parse_budget.saturating_sub(now.elapsed());
                }
            })
            .ok_or_else(|| match timed_out {
                true => anyhow::anyhow!(ParseTimeout),
                false => anyhow::anyhow!("parsing failed"),
            })
    })
}

#[ztracing::instrument(skip_all)]
pub(super) fn get_injections(
    config: &InjectionConfig,
    text: &BufferSnapshot,
    outer_range: Range<Anchor>,
    node: Node,
    language_registry: &Arc<LanguageRegistry>,
    depth: usize,
    changed_ranges: &[Range<usize>],
    combined_injection_ranges: &mut HashMap<LanguageId, (Arc<Language>, Vec<tree_sitter::Range>)>,
    queue: &mut BinaryHeap<ParseStep>,
) {
    let mut query_cursor = QueryCursorHandle::new();
    let mut prev_match = None;

    // Ensure that a `ParseStep` is created for every combined injection language, even
    // if there currently no matches for that injection.
    combined_injection_ranges.clear();
    for pattern in &config.patterns {
        if let (Some(language_name), true) = (pattern.language.as_ref(), pattern.combined)
            && let Some(language) = language_registry
                .language_for_name_or_extension(language_name)
                .now_or_never()
                .and_then(|language| language.ok())
        {
            combined_injection_ranges.insert(language.id, (language, Vec::new()));
        }
    }

    for query_range in changed_ranges {
        query_cursor.set_byte_range(query_range.start.saturating_sub(1)..query_range.end + 1);
        let mut matches = query_cursor.matches(&config.query, node, TextProvider(text.as_rope()));
        while let Some(mat) = matches.next() {
            let content_ranges = mat
                .nodes_for_capture_index(config.content_capture_ix)
                .map(|node| node.range())
                .collect::<Vec<_>>();
            if content_ranges.is_empty() {
                continue;
            }

            let content_range =
                content_ranges.first().unwrap().start_byte..content_ranges.last().unwrap().end_byte;

            // Avoid duplicate matches if two changed ranges intersect the same injection.
            if let Some((prev_pattern_ix, prev_range)) = &prev_match
                && mat.pattern_index == *prev_pattern_ix
                && content_range == *prev_range
            {
                continue;
            }

            prev_match = Some((mat.pattern_index, content_range.clone()));
            let combined = config.patterns[mat.pattern_index].combined;

            let mut step_range = content_range.clone();
            let language_name =
                if let Some(name) = config.patterns[mat.pattern_index].language.as_ref() {
                    Some(Cow::Borrowed(name.as_ref()))
                } else if let Some(language_node) = config
                    .language_capture_ix
                    .and_then(|ix| mat.nodes_for_capture_index(ix).next())
                {
                    step_range.start = cmp::min(content_range.start, language_node.start_byte());
                    step_range.end = cmp::max(content_range.end, language_node.end_byte());
                    let language_name: String =
                        text.text_for_range(language_node.byte_range()).collect();

                    // Enable paths ending in a language extension to represent a language name: e.g. "foo/bar/baz.rs"
                    if let Some(last_dot_pos) = language_name.rfind('.') {
                        Some(Cow::Owned(language_name[last_dot_pos + 1..].to_string()))
                    } else {
                        Some(Cow::Owned(language_name))
                    }
                } else {
                    None
                };

            if let Some(language_name) = language_name {
                let language = language_registry
                    .language_for_name_or_extension(&language_name)
                    .now_or_never()
                    .and_then(|language| language.ok());
                let range = text.anchor_before(step_range.start)..text.anchor_after(step_range.end);
                if let Some(language) = language {
                    if combined {
                        combined_injection_ranges
                            .entry(language.id)
                            .or_insert_with(|| (language.clone(), vec![]))
                            .1
                            .extend(content_ranges);
                    } else {
                        queue.push(ParseStep {
                            depth,
                            language: ParseStepLanguage::Loaded { language },
                            included_ranges: content_ranges,
                            range,
                            mode: ParseMode::Single,
                        });
                    }
                } else {
                    queue.push(ParseStep {
                        depth,
                        language: ParseStepLanguage::Pending {
                            name: language_name.into(),
                        },
                        included_ranges: content_ranges,
                        range,
                        mode: ParseMode::Single,
                    });
                }
            }
        }
    }

    for (_, (language, mut included_ranges)) in combined_injection_ranges.drain() {
        included_ranges.sort_unstable_by(|a, b| {
            Ord::cmp(&a.start_byte, &b.start_byte).then_with(|| Ord::cmp(&a.end_byte, &b.end_byte))
        });
        queue.push(ParseStep {
            depth,
            language: ParseStepLanguage::Loaded { language },
            range: outer_range.clone(),
            included_ranges,
            mode: ParseMode::Combined {
                parent_layer_range: node.start_byte()..node.end_byte(),
                parent_layer_changed_ranges: changed_ranges.to_vec(),
            },
        })
    }
}

/// Updates the given list of included `ranges`, removing any ranges that intersect
/// `removed_ranges`, and inserting the given `new_ranges`.
///
/// Returns a new vector of ranges, and the range of the vector that was changed,
/// from the previous `ranges` vector.
pub(crate) fn splice_included_ranges(
    mut ranges: Vec<tree_sitter::Range>,
    removed_ranges: &[Range<usize>],
    new_ranges: &[tree_sitter::Range],
) -> (Vec<tree_sitter::Range>, Range<usize>) {
    let mut removed_ranges = removed_ranges.iter().cloned().peekable();
    let mut new_ranges = new_ranges.iter().cloned().peekable();
    let mut ranges_ix = 0;
    let mut changed_portion: Option<Range<usize>> = None;
    loop {
        let next_new_range = new_ranges.peek();
        let next_removed_range = removed_ranges.peek();

        let (remove, insert) = match (next_removed_range, next_new_range) {
            (None, None) => break,
            (Some(_), None) => (removed_ranges.next().unwrap(), None),
            (Some(next_removed_range), Some(next_new_range)) => {
                if next_removed_range.end < next_new_range.start_byte {
                    (removed_ranges.next().unwrap(), None)
                } else {
                    let mut start = next_new_range.start_byte;
                    let mut end = next_new_range.end_byte;

                    while let Some(next_removed_range) = removed_ranges.peek() {
                        if next_removed_range.start > next_new_range.end_byte {
                            break;
                        }
                        let next_removed_range = removed_ranges.next().unwrap();
                        start = cmp::min(start, next_removed_range.start);
                        end = cmp::max(end, next_removed_range.end);
                    }

                    (start..end, Some(new_ranges.next().unwrap()))
                }
            }
            (None, Some(next_new_range)) => (
                next_new_range.start_byte..next_new_range.end_byte,
                Some(new_ranges.next().unwrap()),
            ),
        };

        let mut start_ix = ranges_ix
            + match ranges[ranges_ix..].binary_search_by_key(&remove.start, |r| r.end_byte) {
                Ok(ix) => ix,
                Err(ix) => ix,
            };
        let mut end_ix = ranges_ix
            + match ranges[ranges_ix..].binary_search_by_key(&remove.end, |r| r.start_byte) {
                Ok(ix) => ix + 1,
                Err(ix) => ix,
            };

        // If there are empty ranges, then there may be multiple ranges with the same
        // start or end. Expand the splice to include any adjacent ranges that touch
        // the changed range.
        while start_ix > 0 {
            if ranges[start_ix - 1].end_byte == remove.start {
                start_ix -= 1;
            } else {
                break;
            }
        }
        while let Some(range) = ranges.get(end_ix) {
            if range.start_byte == remove.end {
                end_ix += 1;
            } else {
                break;
            }
        }
        let changed_start = changed_portion
            .as_ref()
            .map_or(usize::MAX, |range| range.start)
            .min(start_ix);
        let changed_end =
            changed_portion
                .as_ref()
                .map_or(0, |range| range.end)
                .max(if insert.is_some() {
                    start_ix + 1
                } else {
                    start_ix
                });
        changed_portion = Some(changed_start..changed_end);

        ranges.splice(start_ix..end_ix, insert);
        ranges_ix = start_ix;
    }

    (ranges, changed_portion.unwrap_or(0..0))
}

/// Ensure there are newline ranges in between content range that appear on
/// different lines. For performance, only iterate through the given range of
/// indices. All of the ranges in the array are relative to a given start byte
/// and point.
#[ztracing::instrument(skip_all)]
pub(super) fn insert_newlines_between_ranges(
    indices: Range<usize>,
    ranges: &mut Vec<tree_sitter::Range>,
    text: &text::BufferSnapshot,
    start_byte: usize,
    start_point: Point,
) {
    let mut ix = indices.end + 1;
    while ix > indices.start {
        ix -= 1;
        if 0 == ix || ix == ranges.len() {
            continue;
        }

        let range_b = ranges[ix];
        let range_a = &mut ranges[ix - 1];
        if range_a.end_point.column == 0 {
            continue;
        }

        if range_a.end_point.row < range_b.start_point.row {
            let end_point = start_point + Point::from_ts_point(range_a.end_point);
            let line_end = Point::new(end_point.row, text.line_len(end_point.row));
            if end_point.column >= line_end.column {
                range_a.end_byte += 1;
                range_a.end_point.row += 1;
                range_a.end_point.column = 0;
            } else {
                let newline_offset = text.point_to_offset(line_end);
                ranges.insert(
                    ix,
                    tree_sitter::Range {
                        start_byte: newline_offset - start_byte,
                        end_byte: newline_offset - start_byte + 1,
                        start_point: (line_end - start_point).to_ts_point(),
                        end_point: ((line_end - start_point) + Point::new(1, 0)).to_ts_point(),
                    },
                )
            }
        }
    }
}

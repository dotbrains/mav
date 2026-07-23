use std::{
    borrow::Borrow,
    cmp::Ordering,
    iter,
    ops::Range,
    sync::atomic::{self, AtomicBool},
};

use gpui::{BackgroundExecutor, SharedString};
use nucleo::Utf32Str;

use crate::{
    Cancelled, Case, LengthPenalty, Query, case_penalty, count_case_mismatches,
    matcher::{self, LENGTH_PENALTY},
    positions_from_sorted,
};
use fuzzy::CharBag;

#[derive(Clone, Debug)]
pub struct StringMatchCandidate {
    pub id: usize,
    pub string: SharedString,
    char_bag: CharBag,
}

impl StringMatchCandidate {
    pub fn new(id: usize, string: impl Into<SharedString>) -> Self {
        Self::from_shared(id, string.into())
    }

    pub fn from_shared(id: usize, string: SharedString) -> Self {
        let char_bag = CharBag::from(string.as_ref());
        Self {
            id,
            string,
            char_bag,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StringMatch {
    pub candidate_id: usize,
    pub score: f64,
    pub positions: Vec<usize>,
    pub string: SharedString,
}

impl StringMatch {
    pub fn ranges(&self) -> impl '_ + Iterator<Item = Range<usize>> {
        let mut positions = self.positions.iter().peekable();
        iter::from_fn(move || {
            let start = *positions.next()?;
            let char_len = self.char_len_at_index(start)?;
            let mut end = start + char_len;
            while let Some(next_start) = positions.peek() {
                if end == **next_start {
                    let Some(char_len) = self.char_len_at_index(end) else {
                        break;
                    };
                    end += char_len;
                    positions.next();
                } else {
                    break;
                }
            }
            Some(start..end)
        })
    }

    fn char_len_at_index(&self, ix: usize) -> Option<usize> {
        self.string
            .get(ix..)
            .and_then(|slice| slice.chars().next().map(|c| c.len_utf8()))
    }
}

impl PartialEq for StringMatch {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for StringMatch {}

impl PartialOrd for StringMatch {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StringMatch {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| self.candidate_id.cmp(&other.candidate_id))
    }
}

pub async fn match_strings_async<T>(
    candidates: &[T],
    query: &str,
    case: Case,
    length_penalty: LengthPenalty,
    max_results: usize,
    cancel_flag: &AtomicBool,
    executor: BackgroundExecutor,
) -> Vec<StringMatch>
where
    T: Borrow<StringMatchCandidate> + Sync,
{
    if candidates.is_empty() || max_results == 0 {
        return Vec::new();
    }

    let Some(query) = Query::build(query, case) else {
        return empty_query_results(candidates, max_results);
    };

    let num_cpus = executor.num_cpus().min(candidates.len());
    let base_size = candidates.len() / num_cpus;
    let remainder = candidates.len() % num_cpus;
    let mut segment_results = (0..num_cpus)
        .map(|_| Vec::with_capacity(max_results.min(candidates.len())))
        .collect::<Vec<_>>();

    let config = nucleo::Config::DEFAULT;
    let mut matchers = matcher::get_matchers(num_cpus, config);

    executor
        .scoped(|scope| {
            for (segment_idx, (results, matcher)) in segment_results
                .iter_mut()
                .zip(matchers.iter_mut())
                .enumerate()
            {
                let query = &query;
                scope.spawn(async move {
                    let segment_start = segment_idx * base_size + segment_idx.min(remainder);
                    let segment_end =
                        (segment_idx + 1) * base_size + (segment_idx + 1).min(remainder);

                    match_string_helper(
                        &candidates[segment_start..segment_end],
                        query,
                        matcher,
                        length_penalty,
                        results,
                        cancel_flag,
                    )
                    .ok();
                });
            }
        })
        .await;

    matcher::return_matchers(matchers);

    if cancel_flag.load(atomic::Ordering::Acquire) {
        return Vec::new();
    }

    let mut results = segment_results.concat();
    util::truncate_to_bottom_n_sorted_by(&mut results, max_results, &|a, b| b.cmp(a));
    results
}

pub fn match_strings<T>(
    candidates: &[T],
    query: &str,
    case: Case,
    length_penalty: LengthPenalty,
    max_results: usize,
) -> Vec<StringMatch>
where
    T: Borrow<StringMatchCandidate>,
{
    if candidates.is_empty() || max_results == 0 {
        return Vec::new();
    }

    let Some(query) = Query::build(query, case) else {
        return empty_query_results(candidates, max_results);
    };

    let config = nucleo::Config::DEFAULT;
    let mut matcher = matcher::get_matcher(config);
    let mut results = Vec::with_capacity(max_results.min(candidates.len()));

    match_string_helper(
        candidates,
        &query,
        &mut matcher,
        length_penalty,
        &mut results,
        &AtomicBool::new(false),
    )
    .ok();

    matcher::return_matcher(matcher);
    util::truncate_to_bottom_n_sorted_by(&mut results, max_results, &|a, b| b.cmp(a));
    results
}

fn empty_query_results<T: Borrow<StringMatchCandidate>>(
    candidates: &[T],
    max_results: usize,
) -> Vec<StringMatch> {
    candidates
        .iter()
        .take(max_results)
        .map(|candidate| {
            let borrowed = candidate.borrow();
            StringMatch {
                candidate_id: borrowed.id,
                score: 0.,
                positions: Vec::new(),
                string: borrowed.string.clone(),
            }
        })
        .collect()
}

fn match_string_helper<T>(
    candidates: &[T],
    query: &Query,
    matcher: &mut nucleo::Matcher,
    length_penalty: LengthPenalty,
    results: &mut Vec<StringMatch>,
    cancel_flag: &AtomicBool,
) -> Result<(), Cancelled>
where
    T: Borrow<StringMatchCandidate>,
{
    let mut buf = Vec::new();
    let mut matched_chars: Vec<u32> = Vec::new();
    let mut candidate_chars: Vec<char> = Vec::new();

    for candidate in candidates {
        buf.clear();
        matched_chars.clear();
        if cancel_flag.load(atomic::Ordering::Relaxed) {
            return Err(Cancelled);
        }

        let borrowed = candidate.borrow();

        if !borrowed.char_bag.is_superset(query.char_bag) {
            continue;
        }

        let haystack: Utf32Str = Utf32Str::new(borrowed.string.as_ref(), &mut buf);

        let Some(score) = query.pattern.indices(haystack, matcher, &mut matched_chars) else {
            continue;
        };

        let case_mismatches = count_case_mismatches(
            query.query_chars.as_deref(),
            &matched_chars,
            borrowed.string.as_ref(),
            &mut candidate_chars,
        );

        matched_chars.sort_unstable();
        matched_chars.dedup();

        let positive = score as f64 * case_penalty(case_mismatches);
        let adjusted_score =
            positive - length_penalty_for(borrowed.string.as_ref(), length_penalty);
        let positions = positions_from_sorted(borrowed.string.as_ref(), &matched_chars);

        results.push(StringMatch {
            candidate_id: borrowed.id,
            score: adjusted_score,
            positions,
            string: borrowed.string.clone(),
        });
    }
    Ok(())
}

#[inline]
fn length_penalty_for(s: &str, length_penalty: LengthPenalty) -> f64 {
    if length_penalty.is_on() {
        s.len() as f64 * LENGTH_PENALTY
    } else {
        0.0
    }
}

#[cfg(test)]
#[path = "strings/tests.rs"]
mod tests;

use std::{
    borrow::Borrow,
    sync::atomic::{self, AtomicBool},
};

use crate::{CharBag, char_bag::simple_lowercase};

const BASE_DISTANCE_PENALTY: f64 = 0.6;
const ADDITIONAL_DISTANCE_PENALTY: f64 = 0.05;
const MIN_DISTANCE_PENALTY: f64 = 0.2;

// TODO:
// Use `Path` instead of `&str` for paths.
pub struct Matcher<'a> {
    query: &'a [char],
    lowercase_query: &'a [char],
    query_char_bag: CharBag,
    smart_case: bool,
    penalize_length: bool,
    min_score: f64,
    match_positions: Vec<usize>,
    last_positions: Vec<usize>,
    score_matrix: Vec<Option<f64>>,
    best_position_matrix: Vec<usize>,
}

pub trait MatchCandidate {
    fn has_chars(&self, bag: CharBag) -> bool;
    fn candidate_chars(&self) -> impl Iterator<Item = char>;
}

impl<'a> Matcher<'a> {
    pub fn new(
        query: &'a [char],
        lowercase_query: &'a [char],
        query_char_bag: CharBag,
        smart_case: bool,
        penalize_length: bool,
    ) -> Self {
        Self {
            query,
            lowercase_query,
            query_char_bag,
            min_score: 0.0,
            last_positions: vec![0; lowercase_query.len()],
            match_positions: vec![0; query.len()],
            score_matrix: Vec::new(),
            best_position_matrix: Vec::new(),
            smart_case,
            penalize_length,
        }
    }

    /// Filter and score fuzzy match candidates. Results are returned unsorted, in the same order as
    /// the input candidates.
    pub(crate) fn match_candidates<C, R, F, T>(
        &mut self,
        prefix: &[char],
        lowercase_prefix: &[char],
        candidates: impl Iterator<Item = T>,
        results: &mut Vec<R>,
        cancel_flag: &AtomicBool,
        build_match: F,
    ) where
        C: MatchCandidate,
        T: Borrow<C>,
        F: Fn(&C, f64, &Vec<usize>) -> R,
    {
        let mut candidate_chars = Vec::new();
        let mut lowercase_candidate_chars = Vec::new();

        for candidate in candidates {
            if !candidate.borrow().has_chars(self.query_char_bag) {
                continue;
            }

            if cancel_flag.load(atomic::Ordering::Acquire) {
                break;
            }

            candidate_chars.clear();
            lowercase_candidate_chars.clear();
            for c in candidate.borrow().candidate_chars() {
                candidate_chars.push(c);
                lowercase_candidate_chars.push(simple_lowercase(c));
            }

            if !self.find_last_positions(lowercase_prefix, &lowercase_candidate_chars) {
                continue;
            }

            let matrix_len =
                self.query.len() * (lowercase_prefix.len() + lowercase_candidate_chars.len());
            self.score_matrix.clear();
            self.score_matrix.resize(matrix_len, None);
            self.best_position_matrix.clear();
            self.best_position_matrix.resize(matrix_len, 0);

            let score = self.score_match(
                &candidate_chars,
                &lowercase_candidate_chars,
                prefix,
                lowercase_prefix,
            );

            if score > 0.0 {
                results.push(build_match(
                    candidate.borrow(),
                    score,
                    &self.match_positions,
                ));
            }
        }
    }

    fn find_last_positions(
        &mut self,
        lowercase_prefix: &[char],
        lowercase_candidate: &[char],
    ) -> bool {
        let mut lowercase_prefix = lowercase_prefix.iter();
        let mut lowercase_candidate = lowercase_candidate.iter();
        for (i, char) in self.lowercase_query.iter().enumerate().rev() {
            if let Some(j) = lowercase_candidate.rposition(|c| c == char) {
                self.last_positions[i] = j + lowercase_prefix.len();
            } else if let Some(j) = lowercase_prefix.rposition(|c| c == char) {
                self.last_positions[i] = j;
            } else {
                return false;
            }
        }
        true
    }

    fn score_match(
        &mut self,
        path: &[char],
        path_lowercased: &[char],
        prefix: &[char],
        lowercase_prefix: &[char],
    ) -> f64 {
        let score = self.recursive_score_match(
            path,
            path_lowercased,
            prefix,
            lowercase_prefix,
            0,
            0,
            self.query.len() as f64,
        ) * self.query.len() as f64;

        if score <= 0.0 {
            return 0.0;
        }
        let path_len = prefix.len() + path.len();
        let mut cur_start = 0;
        let mut byte_ix = 0;
        let mut char_ix = 0;
        for i in 0..self.query.len() {
            let match_char_ix = self.best_position_matrix[i * path_len + cur_start];
            while char_ix < match_char_ix {
                let ch = prefix
                    .get(char_ix)
                    .or_else(|| path.get(char_ix - prefix.len()))
                    .unwrap();
                byte_ix += ch.len_utf8();
                char_ix += 1;
            }

            self.match_positions[i] = byte_ix;

            let matched_ch = prefix
                .get(match_char_ix)
                .or_else(|| path.get(match_char_ix - prefix.len()))
                .unwrap();
            byte_ix += matched_ch.len_utf8();

            cur_start = match_char_ix + 1;
            char_ix = match_char_ix + 1;
        }

        score
    }

    fn recursive_score_match(
        &mut self,
        path: &[char],
        path_lowercased: &[char],
        prefix: &[char],
        lowercase_prefix: &[char],
        query_idx: usize,
        path_idx: usize,
        cur_score: f64,
    ) -> f64 {
        if query_idx == self.query.len() {
            return 1.0;
        }

        let limit = self.last_positions[query_idx];
        let max_valid_index = (prefix.len() + path_lowercased.len()).saturating_sub(1);
        let safe_limit = limit.min(max_valid_index);

        if path_idx > safe_limit {
            return 0.0;
        }

        let path_len = prefix.len() + path.len();
        if let Some(memoized) = self.score_matrix[query_idx * path_len + path_idx] {
            return memoized;
        }

        let mut score = 0.0;
        let mut best_position = 0;

        let query_char = self.lowercase_query[query_idx];

        let mut last_slash = 0;

        for j in path_idx..=safe_limit {
            let path_char = if j < prefix.len() {
                lowercase_prefix[j]
            } else {
                let path_index = j - prefix.len();
                match path_lowercased.get(path_index) {
                    Some(&char) => char,
                    None => continue,
                }
            };
            let is_path_sep = path_char == '/';

            if query_idx == 0 && is_path_sep {
                last_slash = j;
            }
            let need_to_score = query_char == path_char || (is_path_sep && query_char == '_');
            if need_to_score {
                let curr = match prefix.get(j) {
                    Some(&curr) => curr,
                    None => path[j - prefix.len()],
                };

                let mut char_score = 1.0;
                if j > path_idx {
                    let last = match prefix.get(j - 1) {
                        Some(&last) => last,
                        None => path[j - 1 - prefix.len()],
                    };

                    if last == '/' {
                        char_score = 0.9;
                    } else if (last == '-' || last == '_' || last == ' ' || last.is_numeric())
                        || (last.is_lowercase() && curr.is_uppercase())
                    {
                        char_score = 0.8;
                    } else if last == '.' {
                        char_score = 0.7;
                    } else if query_idx == 0 {
                        char_score = BASE_DISTANCE_PENALTY;
                    } else {
                        char_score = MIN_DISTANCE_PENALTY.max(
                            BASE_DISTANCE_PENALTY
                                - (j - path_idx - 1) as f64 * ADDITIONAL_DISTANCE_PENALTY,
                        );
                    }
                }

                // Apply a severe penalty if the case doesn't match.
                // This will make the exact matches have higher score than the case-insensitive and the
                // path insensitive matches.
                if (self.smart_case || curr == '/') && self.query[query_idx] != curr {
                    char_score *= 0.001;
                }

                let mut multiplier = char_score;

                // Scale the score based on how deep within the path we found the match.
                if self.penalize_length && query_idx == 0 {
                    multiplier /= ((prefix.len() + path.len()) - last_slash) as f64;
                }

                let mut next_score = 1.0;
                if self.min_score > 0.0 {
                    next_score = cur_score * multiplier;
                    // Scores only decrease. If we can't pass the previous best, bail
                    if next_score < self.min_score {
                        // Ensure that score is non-zero so we use it in the memo table.
                        if score == 0.0 {
                            score = 1e-18;
                        }
                        continue;
                    }
                }

                let new_score = self.recursive_score_match(
                    path,
                    path_lowercased,
                    prefix,
                    lowercase_prefix,
                    query_idx + 1,
                    j + 1,
                    next_score,
                ) * multiplier;

                if new_score > score {
                    score = new_score;
                    best_position = j;
                    // Optimization: can't score better than 1.
                    if new_score == 1.0 {
                        break;
                    }
                }
            }
        }

        if best_position != 0 {
            self.best_position_matrix[query_idx * path_len + path_idx] = best_position;
        }

        self.score_matrix[query_idx * path_len + path_idx] = Some(score);
        score
    }
}

#[cfg(test)]
#[path = "matcher/tests.rs"]
mod tests;

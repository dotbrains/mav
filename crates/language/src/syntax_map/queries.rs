use crate::Grammar;
use std::{
    cmp::Reverse,
    ops::{DerefMut, Range},
};
use streaming_iterator::StreamingIterator;
use text::Rope;
use tree_sitter::{Query, QueryCursor, QueryMatch, QueryPredicateArg};

use super::types::*;

impl<'a> SyntaxMapCaptures<'a> {
    pub(super) fn new(
        range: Range<usize>,
        text: &'a Rope,
        layers: impl Iterator<Item = SyntaxLayer<'a>>,
        query: fn(&Grammar) -> Option<&Query>,
    ) -> Self {
        let mut result = Self {
            layers: Vec::new(),
            grammars: Vec::new(),
            active_layer_count: 0,
        };
        for layer in layers {
            let grammar = match &layer.language.grammar {
                Some(grammar) => grammar,
                None => continue,
            };
            let query = match query(grammar) {
                Some(query) => query,
                None => continue,
            };

            let mut query_cursor = QueryCursorHandle::new();

            // TODO - add a Tree-sitter API to remove the need for this.
            let cursor = unsafe {
                std::mem::transmute::<&mut tree_sitter::QueryCursor, &'static mut QueryCursor>(
                    query_cursor.deref_mut(),
                )
            };

            cursor.set_byte_range(range.clone());
            let captures = cursor.captures(query, layer.node(), TextProvider(text));
            let grammar_index = result
                .grammars
                .iter()
                .position(|g| g.id() == grammar.id())
                .unwrap_or_else(|| {
                    result.grammars.push(grammar);
                    result.grammars.len() - 1
                });
            let mut layer = SyntaxMapCapturesLayer {
                depth: layer.depth,
                grammar_index,
                next_capture: None,
                captures,
                _query_cursor: query_cursor,
            };

            layer.advance();
            if layer.next_capture.is_some() {
                let key = layer.sort_key();
                let ix = match result.layers[..result.active_layer_count]
                    .binary_search_by_key(&key, |layer| layer.sort_key())
                {
                    Ok(ix) | Err(ix) => ix,
                };
                result.layers.insert(ix, layer);
                result.active_layer_count += 1;
            } else {
                result.layers.push(layer);
            }
        }

        result
    }

    pub fn grammars(&self) -> &[&'a Grammar] {
        &self.grammars
    }

    pub fn peek(&self) -> Option<SyntaxMapCapture<'a>> {
        let layer = self.layers[..self.active_layer_count].first()?;
        let capture = layer.next_capture?;
        Some(SyntaxMapCapture {
            grammar_index: layer.grammar_index,
            index: capture.index,
            node: capture.node,
        })
    }

    pub fn advance(&mut self) -> bool {
        let layer = if let Some(layer) = self.layers[..self.active_layer_count].first_mut() {
            layer
        } else {
            return false;
        };

        layer.advance();
        if layer.next_capture.is_some() {
            let key = layer.sort_key();
            let i = 1 + self.layers[1..self.active_layer_count]
                .iter()
                .position(|later_layer| key < later_layer.sort_key())
                .unwrap_or(self.active_layer_count - 1);
            self.layers[0..i].rotate_left(1);
        } else {
            self.layers[0..self.active_layer_count].rotate_left(1);
            self.active_layer_count -= 1;
        }

        true
    }

    pub fn set_byte_range(&mut self, range: Range<usize>) {
        for layer in &mut self.layers {
            layer.captures.set_byte_range(range.clone());
            if let Some(capture) = &layer.next_capture
                && capture.node.end_byte() > range.start
            {
                continue;
            }
            layer.advance();
        }
        self.layers.sort_unstable_by_key(|layer| layer.sort_key());
        self.active_layer_count = self
            .layers
            .iter()
            .position(|layer| layer.next_capture.is_none())
            .unwrap_or(self.layers.len());
    }
}

#[derive(Default)]
pub struct TreeSitterOptions {
    pub max_start_depth: Option<u32>,
    pub max_bytes_to_query: Option<usize>,
}

impl TreeSitterOptions {
    pub fn max_start_depth(max_start_depth: u32) -> Self {
        Self {
            max_start_depth: Some(max_start_depth),
            max_bytes_to_query: None,
        }
    }
}

impl<'a> SyntaxMapMatches<'a> {
    pub(super) fn new(
        range: Range<usize>,
        text: &'a Rope,
        layers: impl Iterator<Item = SyntaxLayer<'a>>,
        query: fn(&Grammar) -> Option<&Query>,
        options: TreeSitterOptions,
    ) -> Self {
        let mut result = Self::default();
        for layer in layers {
            let grammar = match &layer.language.grammar {
                Some(grammar) => grammar,
                None => continue,
            };
            let query = match query(grammar) {
                Some(query) => query,
                None => continue,
            };

            let mut query_cursor = QueryCursorHandle::new();

            // TODO - add a Tree-sitter API to remove the need for this.
            let cursor = unsafe {
                std::mem::transmute::<&mut tree_sitter::QueryCursor, &'static mut QueryCursor>(
                    query_cursor.deref_mut(),
                )
            };
            cursor.set_max_start_depth(options.max_start_depth);

            if let Some(max_bytes_to_query) = options.max_bytes_to_query {
                let midpoint = (range.start + range.end) / 2;
                let containing_range_start = midpoint.saturating_sub(max_bytes_to_query / 2);
                let containing_range_end =
                    containing_range_start.saturating_add(max_bytes_to_query);
                cursor.set_containing_byte_range(containing_range_start..containing_range_end);
            }

            cursor.set_byte_range(range.clone());
            let matches = cursor.matches(query, layer.node(), TextProvider(text));
            let grammar_index = result
                .grammars
                .iter()
                .position(|g| g.id() == grammar.id())
                .unwrap_or_else(|| {
                    result.grammars.push(grammar);
                    result.grammars.len() - 1
                });
            let mut layer = SyntaxMapMatchesLayer {
                language: layer.language.clone(),
                depth: layer.depth,
                grammar_index,
                matches,
                query,
                next_pattern_index: 0,
                next_captures: Vec::new(),
                has_next: false,
                _query_cursor: query_cursor,
            };

            layer.advance();
            if layer.has_next {
                let key = layer.sort_key();
                let ix = match result.layers[..result.active_layer_count]
                    .binary_search_by_key(&key, |layer| layer.sort_key())
                {
                    Ok(ix) | Err(ix) => ix,
                };
                result.layers.insert(ix, layer);
                result.active_layer_count += 1;
            } else {
                result.layers.push(layer);
            }
        }
        result
    }

    pub fn grammars(&self) -> &[&'a Grammar] {
        &self.grammars
    }

    pub fn peek(&self) -> Option<SyntaxMapMatch<'_>> {
        let layer = self.layers.first()?;

        if !layer.has_next {
            return None;
        }

        Some(SyntaxMapMatch {
            language: layer.language.clone(),
            depth: layer.depth,
            grammar_index: layer.grammar_index,
            pattern_index: layer.next_pattern_index,
            captures: &layer.next_captures,
        })
    }

    pub fn advance(&mut self) -> bool {
        let layer = if let Some(layer) = self.layers.first_mut() {
            layer
        } else {
            return false;
        };

        layer.advance();
        if layer.has_next {
            let key = layer.sort_key();
            let i = 1 + self.layers[1..self.active_layer_count]
                .iter()
                .position(|later_layer| key < later_layer.sort_key())
                .unwrap_or(self.active_layer_count - 1);
            self.layers[0..i].rotate_left(1);
        } else if self.active_layer_count != 0 {
            self.layers[0..self.active_layer_count].rotate_left(1);
            self.active_layer_count -= 1;
        }

        true
    }

    // pub fn set_byte_range(&mut self, range: Range<usize>) {
    //     for layer in &mut self.layers {
    //         layer.matches.set_byte_range(range.clone());
    //         layer.advance();
    //     }
    //     self.layers.sort_unstable_by_key(|layer| layer.sort_key());
    //     self.active_layer_count = self
    //         .layers
    //         .iter()
    //         .position(|layer| !layer.has_next)
    //         .unwrap_or(self.layers.len());
    // }
}

impl SyntaxMapCapturesLayer<'_> {
    fn advance(&mut self) {
        self.next_capture = self.captures.next().map(|(mat, ix)| mat.captures[*ix]);
    }

    fn sort_key(&self) -> (usize, Reverse<usize>, usize) {
        if let Some(capture) = &self.next_capture {
            let range = capture.node.byte_range();
            (range.start, Reverse(range.end), self.depth)
        } else {
            (usize::MAX, Reverse(0), usize::MAX)
        }
    }
}

impl SyntaxMapMatchesLayer<'_> {
    fn advance(&mut self) {
        loop {
            if let Some(mat) = self.matches.next() {
                if !satisfies_custom_predicates(self.query, mat) {
                    continue;
                }
                self.next_captures.clear();
                self.next_captures.extend_from_slice(mat.captures);
                self.next_pattern_index = mat.pattern_index;
                self.has_next = true;
                return;
            } else {
                self.has_next = false;
                return;
            }
        }
    }

    fn sort_key(&self) -> (usize, Reverse<usize>, usize) {
        if self.has_next {
            let captures = &self.next_captures;
            if let Some((first, last)) = captures.first().zip(captures.last()) {
                return (
                    first.node.start_byte(),
                    Reverse(last.node.end_byte()),
                    self.depth,
                );
            }
        }
        (usize::MAX, Reverse(0), usize::MAX)
    }
}

impl<'a> Iterator for SyntaxMapCaptures<'a> {
    type Item = SyntaxMapCapture<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let result = self.peek();
        self.advance();
        result
    }
}

fn satisfies_custom_predicates(query: &Query, mat: &QueryMatch) -> bool {
    for predicate in query.general_predicates(mat.pattern_index) {
        let satisfied = match predicate.operator.as_ref() {
            "has-parent?" => has_parent(&predicate.args, mat),
            "not-has-parent?" => !has_parent(&predicate.args, mat),
            _ => true,
        };
        if !satisfied {
            return false;
        }
    }
    true
}

fn has_parent(args: &[QueryPredicateArg], mat: &QueryMatch) -> bool {
    let (
        Some(QueryPredicateArg::Capture(capture_ix)),
        Some(QueryPredicateArg::String(parent_kind)),
    ) = (args.first(), args.get(1))
    else {
        return false;
    };

    let Some(capture) = mat.captures.iter().find(|c| c.index == *capture_ix) else {
        return false;
    };

    capture
        .node
        .parent()
        .is_some_and(|p| p.kind() == parent_kind.as_ref())
}

pub(super) fn join_ranges(
    a: impl Iterator<Item = Range<usize>>,
    b: impl Iterator<Item = Range<usize>>,
) -> Vec<Range<usize>> {
    let mut result = Vec::<Range<usize>>::new();
    let mut a = a.peekable();
    let mut b = b.peekable();
    loop {
        let range = match (a.peek(), b.peek()) {
            (Some(range_a), Some(range_b)) => {
                if range_a.start < range_b.start {
                    a.next().unwrap()
                } else {
                    b.next().unwrap()
                }
            }
            (None, Some(_)) => b.next().unwrap(),
            (Some(_), None) => a.next().unwrap(),
            (None, None) => break,
        };

        if let Some(last) = result.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
            continue;
        }
        result.push(range);
    }
    result
}

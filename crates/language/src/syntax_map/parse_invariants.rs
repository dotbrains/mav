use super::*;
use crate::LanguageId;
use std::{cmp::Ordering, ops::Range};
use text::{Anchor, BufferSnapshot};

impl SyntaxSnapshot {
    #[cfg(debug_assertions)]
    pub(super) fn check_invariants(&self, text: &BufferSnapshot) {
        let mut max_depth = 0;
        let mut prev_layer: Option<(Range<Anchor>, Option<LanguageId>)> = None;
        for layer in self.layers.iter() {
            match Ord::cmp(&layer.depth, &max_depth) {
                Ordering::Less => {
                    panic!("layers out of order")
                }
                Ordering::Equal => {
                    if let Some((prev_range, prev_language_id)) = prev_layer {
                        match layer.range.start.cmp(&prev_range.start, text) {
                            Ordering::Less => panic!("layers out of order"),
                            Ordering::Equal => match layer.range.end.cmp(&prev_range.end, text) {
                                Ordering::Less => panic!("layers out of order"),
                                Ordering::Equal => {
                                    if layer.content.language_id() < prev_language_id {
                                        panic!("layers out of order")
                                    }
                                }
                                Ordering::Greater => {}
                            },
                            Ordering::Greater => {}
                        }
                    }
                    prev_layer = Some((layer.range.clone(), layer.content.language_id()));
                }
                Ordering::Greater => {
                    prev_layer = None;
                }
            }

            max_depth = layer.depth;
        }
    }
}

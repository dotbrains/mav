use super::*;
use buffer_diff::{DiffHunkStatus, DiffHunkStatusKind};
use gpui::{App, Entity, TestAppContext};
use indoc::indoc;
use language::{Buffer, Rope};
use parking_lot::RwLock;
use rand::prelude::*;
use settings::SettingsStore;
use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};
use util::RandomCharIter;
use util::rel_path::rel_path;
use util::test::sample_text;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

mod anchor_boundary_tests;
mod basic_diff_tests;
mod chunk_bitmap_tests;
mod diff_hunk_tests;
mod empty_anchor_tests;
mod excerpt_lifecycle_tests;
mod excerpt_range_tests;
mod inverted_diff_tests;
mod map_excerpt_tests;
mod multi_excerpt_diff_tests;
mod path_replacement_tests;
mod random_multibuffer_assertions;
mod random_multibuffer_tests;
mod random_set_range_tests;
mod range_mapping_tests;
mod reference_expected_content;
mod reference_multibuffer;
mod singleton_tests;
mod snapshot_assertions;
mod tail_behavior_tests;
mod title_tests;
mod word_diff_tests;

pub(super) use random_multibuffer_assertions::*;
use reference_multibuffer::*;
pub(super) use snapshot_assertions::*;

fn mutate_excerpt_ranges(
    rng: &mut StdRng,
    existing_ranges: &mut Vec<Range<Point>>,
    buffer: &BufferSnapshot,
    operations: u32,
) {
    let mut ranges_to_add = Vec::new();

    for _ in 0..operations {
        match rng.random_range(0..5) {
            0..=1 if !existing_ranges.is_empty() => {
                let index = rng.random_range(0..existing_ranges.len());
                log::info!("Removing excerpt at index {index}");
                existing_ranges.remove(index);
            }
            _ => {
                let end_row = rng.random_range(0..=buffer.max_point().row);
                let start_row = rng.random_range(0..=end_row);
                let end_col = buffer.line_len(end_row);
                log::info!(
                    "Inserting excerpt for buffer {:?}, row range {:?}",
                    buffer.remote_id(),
                    start_row..end_row
                );
                ranges_to_add.push(Point::new(start_row, 0)..Point::new(end_row, end_col));
            }
        }
    }

    existing_ranges.extend(ranges_to_add);
    existing_ranges.sort_by_key(|r| r.start);
}

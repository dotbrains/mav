//! `ep split-commit` implementation.
//!
//! This command generates a single evaluation example JSON object from a
//! chronologically-ordered unified diff (a "commit").
//!
//! TODO: Port Python code to generate chronologically-ordered commits
use crate::FailedHandling;
use crate::reorder_patch::{
    EditLocation, Patch, PatchLine, edit_locations, extract_edits, locate_edited_line,
};
use crate::word_diff::tokenize;
use anyhow::{Context as _, Result};
use clap::Args;
use edit_prediction::example_spec::ExampleSpec;
use rand::Rng;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use serde::Deserialize;
use similar::{DiffTag, TextDiff};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;

mod command;
mod cursor;
mod generation;
mod human_edits;
mod patch_split;
mod service_files;
#[cfg(test)]
mod tests;
mod types;
mod utf8;

pub use command::run_split_commit;
pub use cursor::{get_cursor_excerpt, sample_cursor_position};
pub use human_edits::imitate_human_edits;
pub use patch_split::{generate_evaluation_example_from_ordered_commit, split_ordered_patch};
pub use types::{
    AnnotatedCommit, CursorPosition, NoMatchingSplitPointError, SplitCommit, SplitCommitArgs,
    SplitPoint, SplitPointKind, SplitPointValue,
};

const MAX_SPLIT_POINT_SAMPLING_ATTEMPTS: usize = 10;
const SAME_FILE_NEAR_LINE_THRESHOLD: usize = 30;

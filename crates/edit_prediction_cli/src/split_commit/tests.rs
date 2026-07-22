use std::path::Path;

use edit_prediction::example_spec::ExampleSpec;

use super::human_edits::{fuzzy_ratio, weighted_select};
use super::patch_split::position_weight;
use super::service_files::{
    edit_starts_on_service_file, has_submodule_gitlink_hunk, is_service_file,
};
use super::types::parse_split_point;
use super::*;

mod cursor;
mod evaluation;
mod filtering;
mod human_edits;
mod patch_split;
mod split_points;

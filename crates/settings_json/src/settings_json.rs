use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{ops::Range, sync::LazyLock};
use tree_sitter::{Query, StreamingIterator as _};
use util::RangeExt;

#[path = "settings_json/array.rs"]
mod array;
#[path = "settings_json/formatting.rs"]
mod formatting;
#[path = "settings_json/helpers.rs"]
mod helpers;
#[path = "settings_json/replace.rs"]
mod replace;
#[cfg(test)]
#[path = "settings_json/tests.rs"]
mod tests;
#[path = "settings_json/update.rs"]
mod update;

pub use array::{
    append_top_level_array_value_in_json_text, replace_top_level_array_value_in_json_text,
};
pub use formatting::{infer_json_indent_size, parse_json_with_comments, to_pretty_json};
pub use replace::replace_value_in_json_text;
pub use update::update_value_in_json_text;

const TS_DOCUMENT_KIND: &str = "document";
const TS_ARRAY_KIND: &str = "array";
const TS_COMMENT_KIND: &str = "comment";

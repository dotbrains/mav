use super::*;
use serde_json::{Value, json};
use unindent::Unindent;

#[path = "tests/array_append.rs"]
mod array_append;
#[path = "tests/array_replace.rs"]
mod array_replace;
#[path = "tests/indent.rs"]
mod indent;
#[path = "tests/object_replace.rs"]
mod object_replace;
#[path = "tests/object_replace_array.rs"]
mod object_replace_array;
#[path = "tests/object_replace_array_nested.rs"]
mod object_replace_array_nested;

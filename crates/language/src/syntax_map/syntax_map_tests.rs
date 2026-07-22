use super::*;
use crate::{
    LanguageConfig, LanguageMatcher, LanguageQueries, buffer_tests::markdown_inline_lang,
    markdown_lang, rust_lang,
};
use gpui::App;
use pretty_assertions::assert_eq;
use rand::rngs::StdRng;
use std::borrow::Cow;
use std::{env, ops::Range, sync::Arc};
use text::{Buffer, BufferId, ReplicaId};
use tree_sitter::Node;
use unindent::Unindent as _;
use util::test::marked_text_ranges;
mod assertions;
mod combined_tests;
mod injection_tests;
mod languages;
mod layer_tests;
mod random_tests;
mod range_tests;
mod shared;

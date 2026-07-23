use collections::{BTreeMap, HashMap, HashSet};
use gpui::SharedString;
use linkify::LinkFinder;
pub use pulldown_cmark::TagEnd as MarkdownTagEnd;
use pulldown_cmark::{
    Alignment, CowStr, HeadingLevel, LinkType, MetadataBlockKind, Options, Parser,
};
use std::{ops::Range, sync::Arc};
use util::markdown::generate_heading_slug;

use crate::{html, path_range::PathWithRange};

mod code_ranges;
mod footnotes;
mod headings;
mod html_tags;
mod links;
mod metadata;
mod runtime;
mod state;
mod types;

pub(crate) use code_ranges::extract_code_block_content_range;
use code_ranges::extract_code_content_range;
use footnotes::build_footnote_definitions;
use headings::build_heading_slugs;
use html_tags::is_br_tag;
pub use links::parse_links_only;
use metadata::parse_metadata_table_rows;
#[cfg(test)]
use metadata::trim_metadata_range;
pub(crate) use runtime::parse_markdown_with_options;
use state::ParseState;
pub use types::{CodeBlockKind, CodeBlockMetadata, MarkdownEvent, MarkdownTag};
pub(crate) use types::{MetadataRow, ParsedMarkdownData, ParsedMetadataBlock};

pub const PARSE_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_SMART_PUNCTUATION)
    .union(Options::ENABLE_HEADING_ATTRIBUTES)
    .union(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS)
    .union(Options::ENABLE_OLD_FOOTNOTES)
    .union(Options::ENABLE_GFM)
    .union(Options::ENABLE_SUPERSCRIPT)
    .union(Options::ENABLE_SUBSCRIPT);

#[cfg(test)]
mod basic_tests;
#[cfg(test)]
mod code_tests;
#[cfg(test)]
mod footnote_link_heading_tests;
#[cfg(test)]
mod html_tests;
#[cfg(test)]
mod options_metadata_tests;

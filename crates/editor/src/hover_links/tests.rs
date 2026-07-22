use super::*;
use crate::{
    DisplayPoint,
    display_map::ToDisplayPoint,
    editor_tests::init_test,
    inlays::inlay_hints::tests::{cached_hint_labels, visible_hint_labels},
    test::editor_lsp_test_context::EditorLspTestContext,
};
use futures::StreamExt;
use gpui::{Modifiers, MousePressureEvent, PressureStage};
use indoc::indoc;
use lsp::request::{GotoDefinition, GotoTypeDefinition};
use multi_buffer::MultiBufferOffset;
use settings::InlayHintSettingsContent;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use util::{assert_set_eq, path};
use workspace::item::Item;

#[path = "test_parse_uri_fragment_position.rs"]
mod test_parse_uri_fragment_position;

#[path = "test_document_link_target_to_hover_link_file_uri_with_fragment.rs"]
mod test_document_link_target_to_hover_link_file_uri_with_fragment;

#[path = "test_document_link_target_to_hover_link_http_url.rs"]
mod test_document_link_target_to_hover_link_http_url;

#[path = "test_hover_type_links.rs"]
mod test_hover_type_links;

#[path = "test_go_to_definition_link_dedup.rs"]
mod test_go_to_definition_link_dedup;

#[path = "test_go_to_definition_link_dedup_no_link.rs"]
mod test_go_to_definition_link_dedup_no_link;

#[path = "test_hover_links.rs"]
mod test_hover_links;

#[path = "test_inlay_hover_links.rs"]
mod test_inlay_hover_links;

#[path = "test_urls.rs"]
mod test_urls;

#[path = "test_hover_preconditions.rs"]
mod test_hover_preconditions;

#[path = "test_urls_at_beginning_of_buffer.rs"]
mod test_urls_at_beginning_of_buffer;

#[path = "test_urls_at_end_of_buffer.rs"]
mod test_urls_at_end_of_buffer;

#[path = "test_link_pattern_file_candidates.rs"]
mod test_link_pattern_file_candidates;

#[path = "test_surrounding_filename.rs"]
mod test_surrounding_filename;

#[path = "test_hover_filenames.rs"]
mod test_hover_filenames;

#[path = "test_hover_filename_with_row_column.rs"]
mod test_hover_filename_with_row_column;

#[path = "test_hover_filename_with_row_only.rs"]
mod test_hover_filename_with_row_only;

#[path = "test_hover_filename_with_non_numeric_suffix.rs"]
mod test_hover_filename_with_non_numeric_suffix;

#[path = "test_hover_markdown_link_with_row_column.rs"]
mod test_hover_markdown_link_with_row_column;

#[path = "test_hover_directories.rs"]
mod test_hover_directories;

#[path = "test_hover_unicode.rs"]
mod test_hover_unicode;

#[path = "test_pressure_links.rs"]
mod test_pressure_links;

#[path = "test_document_links.rs"]
mod test_document_links;

#[path = "test_document_links_take_priority_over_url_detection.rs"]
mod test_document_links_take_priority_over_url_detection;

#[path = "test_cmd_hover_aggregates_document_link_and_definition.rs"]
mod test_cmd_hover_aggregates_document_link_and_definition;

#[path = "test_document_link_tooltip_popover.rs"]
mod test_document_link_tooltip_popover;

#[path = "test_document_link_resolve_on_hover.rs"]
mod test_document_link_resolve_on_hover;

#[path = "test_document_link_tooltip_respects_hover_popover_enabled.rs"]
mod test_document_link_tooltip_respects_hover_popover_enabled;

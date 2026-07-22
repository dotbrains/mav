use super::*;

#[path = "input/basic_settings.rs"]
mod basic_settings;
#[path = "input/comment_actions.rs"]
mod comment_actions;
#[path = "input/delete_actions.rs"]
mod delete_actions;
#[path = "input/entity_input_handler.rs"]
mod entity_input_handler;
#[path = "input/newline_actions.rs"]
mod newline_actions;
#[path = "input/newline_helpers.rs"]
mod newline_helpers;
#[path = "input/snippet_actions.rs"]
mod snippet_actions;
#[path = "input/snippet_insert.rs"]
mod snippet_insert;
#[path = "input/test_support.rs"]
mod test_support;
#[path = "input/text_input.rs"]
mod text_input;
#[path = "input/unwrap_actions.rs"]
mod unwrap_actions;
const ORDERED_LIST_MAX_MARKER_LEN: usize = 16;

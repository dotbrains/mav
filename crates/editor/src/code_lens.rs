use std::sync::Arc;

use collections::{HashMap, HashSet};
use futures::{StreamExt as _, future::join_all, stream::FuturesUnordered};
use gpui::{MouseButton, SharedString, Task, TaskExt, WeakEntity};
use itertools::Itertools;
use language::{BufferId, ClientCommand};
use multi_buffer::{Anchor, MultiBufferRow, MultiBufferSnapshot, ToPoint as _};
use project::{CodeAction, TaskSourceKind, lsp_store::code_lens::CodeLensActions};
use task::TaskContext;
use text::ToOffset as _;

use ui::{Context, Window, div, prelude::*};

use crate::{
    Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT, SelectionEffects,
    actions::ToggleCodeLens,
    display_map::{BlockPlacement, BlockProperties, BlockStyle, CustomBlockId, RenderBlock},
    hover_links::HoverLink,
};

mod commands;
mod render;
mod runtime;
mod types;

pub(super) use commands::try_handle_client_command;
#[cfg(test)]
pub(super) use render::CODE_LENS_SEPARATOR;
use render::{
    build_code_lens_renderer, displayed_title, group_lenses_by_row, rendered_text_matches,
};
pub(super) use types::CodeLensState;
use types::{CodeLensBlock, CodeLensItem, CodeLensLine};

#[cfg(test)]
mod basic_tests;
#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod toggle_tests;
#[cfg(test)]
mod visible_tests;

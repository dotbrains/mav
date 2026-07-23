use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use collections::HashSet;
use futures::StreamExt;
use gpui::TestAppContext;
use indoc::indoc;
use settings::CodeLens;
use util::path;

use multi_buffer::{MultiBufferRow, ToPoint as _};
use text::Point;

use super::{CODE_LENS_SEPARATOR, displayed_title};
use crate::{
    Editor, LSP_REQUEST_DEBOUNCE_TIMEOUT,
    editor_tests::{init_test, update_test_editor_settings},
    test::editor_lsp_test_context::EditorLspTestContext,
};

pub(super) fn code_lens_assertion_text(editor: &Editor, cx: &ui::App) -> String {
    let snapshot = editor.buffer().read(cx).snapshot(cx);
    let mut blocks = editor
        .code_lens
        .as_ref()
        .map(|state| state.blocks.values().flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    blocks.sort_by_key(|block| block.anchor.to_point(&snapshot).row);

    let lens_label = "Lenses";
    let line_label = "Line";
    let mut text = blocks
        .into_iter()
        .map(|block| {
            let row = block.anchor.to_point(&snapshot).row;
            let line_len = snapshot.line_len(MultiBufferRow(row));
            let line_text = snapshot
                .text_for_range(Point::new(row, 0)..Point::new(row, line_len))
                .collect::<String>();
            let lens_text = block
                .line
                .items
                .iter()
                .map(|item| {
                    displayed_title(item)
                        .map(|title| title.to_string())
                        .unwrap_or_else(|| "<placeholder>".to_string())
                })
                .collect::<Vec<_>>()
                .join(CODE_LENS_SEPARATOR);
            let line_number = row + 1;
            let line_label = format!("{line_label} {line_number}");
            let label_width = line_label.len().max(lens_label.len());
            format!(
                "{lens_label:<label_width$}: {lens_text}\n\
                 {line_label:<label_width$}: {line_text}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    text.push('\n');
    text
}

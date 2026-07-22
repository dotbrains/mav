use super::*;
use collections::{HashMap, HashSet};
use editor::{
    DisplayPoint, EditorSettings, Inlay, MultiBufferOffset,
    actions::{GoToDiagnostic, GoToPreviousDiagnostic, Hover, MoveToBeginning},
    display_map::DisplayRow,
    test::{
        editor_content_with_blocks, editor_lsp_test_context::EditorLspTestContext,
        editor_test_context::EditorTestContext,
    },
};
use gpui::{TestAppContext, VisualTestContext};
use indoc::indoc;
use language::{DiagnosticSourceKind, Rope};
use lsp::LanguageServerId;
use pretty_assertions::assert_eq;
use project::{
    FakeFs,
    project_settings::{GoToDiagnosticSeverity, GoToDiagnosticSeverityFilter},
};
use rand::{Rng, rngs::StdRng, seq::IteratorRandom as _};
use serde_json::json;
use settings::SettingsStore;
use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
};
use unindent::Unindent as _;
use util::{RandomCharIter, path, post_inc, rel_path::rel_path};
use workspace::MultiWorkspace;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

mod buffers;
mod navigation;
mod overview;
mod popovers;
mod random_inlays;
mod servers;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        zlog::init_test();
        let settings = SettingsStore::test(cx);
        cx.set_global(settings);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);
        editor::init(cx);
    });
}

fn randomly_update_diagnostics_for_path(
    fs: &FakeFs,
    path: &Path,
    diagnostics: &mut Vec<lsp::Diagnostic>,
    next_id: &mut usize,
    rng: &mut impl Rng,
) {
    let mutation_count = rng.random_range(1..=3);
    for _ in 0..mutation_count {
        if rng.random_bool(0.3) && !diagnostics.is_empty() {
            let idx = rng.random_range(0..diagnostics.len());
            log::info!("  removing diagnostic at index {idx}");
            diagnostics.remove(idx);
        } else {
            let unique_id = *next_id;
            *next_id += 1;

            let new_diagnostic = random_lsp_diagnostic(rng, fs, path, unique_id);

            let ix = rng.random_range(0..=diagnostics.len());
            log::info!(
                "  inserting {} at index {ix}. {},{}..{},{}",
                new_diagnostic.message,
                new_diagnostic.range.start.line,
                new_diagnostic.range.start.character,
                new_diagnostic.range.end.line,
                new_diagnostic.range.end.character,
            );
            for related in new_diagnostic.related_information.iter().flatten() {
                log::info!(
                    "   {}. {},{}..{},{}",
                    related.message,
                    related.location.range.start.line,
                    related.location.range.start.character,
                    related.location.range.end.line,
                    related.location.range.end.character,
                );
            }
            diagnostics.insert(ix, new_diagnostic);
        }
    }
}

fn random_lsp_diagnostic(
    rng: &mut impl Rng,
    fs: &FakeFs,
    path: &Path,
    unique_id: usize,
) -> lsp::Diagnostic {
    // Intentionally allow erroneous ranges some of the time (that run off the end of the file),
    // because language servers can potentially give us those, and we should handle them gracefully.
    const ERROR_MARGIN: usize = 10;

    let file_content = fs.read_file_sync(path).unwrap();
    let file_text = Rope::from(String::from_utf8_lossy(&file_content).as_ref());

    let start = rng.random_range(0..file_text.len().saturating_add(ERROR_MARGIN));
    let end = rng.random_range(start..file_text.len().saturating_add(ERROR_MARGIN));

    let start_point = file_text.offset_to_point_utf16(start);
    let end_point = file_text.offset_to_point_utf16(end);

    let range = lsp::Range::new(
        lsp::Position::new(start_point.row, start_point.column),
        lsp::Position::new(end_point.row, end_point.column),
    );

    let severity = if rng.random_bool(0.5) {
        Some(lsp::DiagnosticSeverity::ERROR)
    } else {
        Some(lsp::DiagnosticSeverity::WARNING)
    };

    let message = format!("diagnostic {unique_id}");

    let related_information = if rng.random_bool(0.3) {
        let info_count = rng.random_range(1..=3);
        let mut related_info = Vec::with_capacity(info_count);

        for i in 0..info_count {
            let info_start = rng.random_range(0..file_text.len().saturating_add(ERROR_MARGIN));
            let info_end =
                rng.random_range(info_start..file_text.len().saturating_add(ERROR_MARGIN));

            let info_start_point = file_text.offset_to_point_utf16(info_start);
            let info_end_point = file_text.offset_to_point_utf16(info_end);

            let info_range = lsp::Range::new(
                lsp::Position::new(info_start_point.row, info_start_point.column),
                lsp::Position::new(info_end_point.row, info_end_point.column),
            );

            related_info.push(lsp::DiagnosticRelatedInformation {
                location: lsp::Location::new(lsp::Uri::from_file_path(path).unwrap(), info_range),
                message: format!("related info {i} for diagnostic {unique_id}"),
            });
        }

        Some(related_info)
    } else {
        None
    };

    lsp::Diagnostic {
        range,
        severity,
        message,
        related_information,
        data: None,
        ..Default::default()
    }
}

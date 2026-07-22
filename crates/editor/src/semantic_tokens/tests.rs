
use std::{
    ops::Range,
    sync::atomic::{self, AtomicUsize},
};

use futures::StreamExt as _;
use gpui::{
    AppContext as _, Entity, Focusable as _, HighlightStyle, TestAppContext, UpdateGlobal as _,
};
use language::{
    Diagnostic, DiagnosticEntry, DiagnosticSet, Language, LanguageAwareStyling, LanguageConfig,
    LanguageMatcher,
};
use languages::FakeLspAdapter;
use lsp::LanguageServerId;
use multi_buffer::{
    AnchorRangeExt, ExpandExcerptDirection, MultiBuffer, MultiBufferOffset, PathKey,
};
use project::Project;
use rope::{Point, PointUtf16};
use serde_json::json;
use settings::{
    GlobalLspSettingsContent, LanguageSettingsContent, SemanticTokenRule, SemanticTokenRules,
    SemanticTokens, SettingsStore,
};
use workspace::{MultiWorkspace, WorkspaceHandle as _};

use crate::{
    Capability,
    editor_tests::{init_test, update_test_language_settings},
    test::{build_editor_with_project, editor_lsp_test_context::EditorLspTestContext},
};

use super::*;

mod capability_core;
mod diagnostics;
mod lifecycle;
mod multibuffer_part;
mod multiserver;
mod restyle;
mod rules;
mod singleton;

fn extract_semantic_highlights(
    editor: &Entity<Editor>,
    cx: &TestAppContext,
) -> Vec<Range<MultiBufferOffset>> {
    editor.read_with(cx, |editor, cx| {
        let multi_buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
        editor
            .display_map
            .read(cx)
            .semantic_token_highlights
            .iter()
            .flat_map(|(_, (v, _))| v.iter())
            .map(|highlights| highlights.range.to_offset(&multi_buffer_snapshot))
            .collect()
    })
}

#[gpui::test]

fn extract_semantic_highlight_styles(
    editor: &Entity<Editor>,
    cx: &TestAppContext,
) -> Vec<HighlightStyle> {
    editor.read_with(cx, |editor, cx| {
        editor
            .display_map
            .read(cx)
            .semantic_token_highlights
            .iter()
            .flat_map(|(_, (v, interner))| v.iter().map(|highlights| interner[highlights.style]))
            .collect()
    })
}

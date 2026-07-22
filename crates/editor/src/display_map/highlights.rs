use super::{
    ChunkRenderer, InlayHighlights, SemanticTokensHighlights, TextHighlights, is_invisible,
    replacement,
};
use crate::EditorStyle;
use collections::IndexSet;
use gpui::{HighlightStyle, Hsla, UnderlineStyle};
use multi_buffer::Anchor;
use project::lsp_store::TokenType;
use std::{iter, ops, ops::Range};
use theme::StatusColors;
use ui::{SharedString, px};
use unicode_segmentation::UnicodeSegmentation;
use ztracing::instrument;

#[derive(Default, Debug)]
pub struct HighlightStyleInterner {
    styles: IndexSet<HighlightStyle>,
}

impl HighlightStyleInterner {
    pub(crate) fn intern(&mut self, style: HighlightStyle) -> HighlightStyleId {
        HighlightStyleId(self.styles.insert_full(style).0 as u32)
    }
}

impl ops::Index<HighlightStyleId> for HighlightStyleInterner {
    type Output = HighlightStyle;

    fn index(&self, index: HighlightStyleId) -> &Self::Output {
        &self.styles[index.0 as usize]
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HighlightStyleId(u32);

/// A `SemanticToken`, but positioned to an offset in a buffer, and stylized.
#[derive(Debug, Clone)]
pub struct SemanticTokenHighlight {
    pub range: Range<Anchor>,
    pub style: HighlightStyleId,
    pub token_type: TokenType,
    pub token_modifiers: u32,
    pub server_id: lsp::LanguageServerId,
}

#[derive(Debug, Default)]
pub struct Highlights<'a> {
    pub text_highlights: Option<&'a TextHighlights>,
    pub inlay_highlights: Option<&'a InlayHighlights>,
    pub semantic_token_highlights: Option<&'a SemanticTokensHighlights>,
    pub styles: HighlightStyles,
}

#[derive(Clone, Copy, Debug)]
pub struct EditPredictionStyles {
    pub insertion: HighlightStyle,
    pub whitespace: HighlightStyle,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct HighlightStyles {
    pub inlay_hint: Option<HighlightStyle>,
    pub edit_prediction: Option<EditPredictionStyles>,
}

#[derive(Clone)]
pub enum ChunkReplacement {
    Renderer(ChunkRenderer),
    Str(SharedString),
}

pub struct HighlightedChunk<'a> {
    pub text: &'a str,
    pub style: Option<HighlightStyle>,
    pub is_tab: bool,
    pub is_inlay: bool,
    pub replacement: Option<ChunkReplacement>,
}

impl<'a> HighlightedChunk<'a> {
    #[instrument(skip_all)]
    pub(crate) fn highlight_invisibles(
        self,
        editor_style: &'a EditorStyle,
    ) -> impl Iterator<Item = Self> + 'a {
        let mut chunks = self.text.graphemes(true).peekable();
        let mut text = self.text;
        let style = self.style;
        let is_tab = self.is_tab;
        let renderer = self.replacement;
        let is_inlay = self.is_inlay;
        iter::from_fn(move || {
            let mut prefix_len = 0;
            while let Some(&chunk) = chunks.peek() {
                let mut chars = chunk.chars();
                let Some(ch) = chars.next() else { break };
                if chunk.len() != ch.len_utf8() || !is_invisible(ch) {
                    prefix_len += chunk.len();
                    chunks.next();
                    continue;
                }
                if prefix_len > 0 {
                    let (prefix, suffix) = text.split_at(prefix_len);
                    text = suffix;
                    return Some(HighlightedChunk {
                        text: prefix,
                        style,
                        is_tab,
                        is_inlay,
                        replacement: renderer.clone(),
                    });
                }
                chunks.next();
                let (prefix, suffix) = text.split_at(chunk.len());
                text = suffix;
                if let Some(replacement) = replacement(ch) {
                    let invisible_highlight = HighlightStyle {
                        background_color: Some(editor_style.status.hint_background),
                        underline: Some(UnderlineStyle {
                            color: Some(editor_style.status.hint),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..Default::default()
                    };
                    let invisible_style = if let Some(style) = style {
                        style.highlight(invisible_highlight)
                    } else {
                        invisible_highlight
                    };
                    return Some(HighlightedChunk {
                        text: prefix,
                        style: Some(invisible_style),
                        is_tab: false,
                        is_inlay,
                        replacement: Some(ChunkReplacement::Str(replacement.into())),
                    });
                } else {
                    let invisible_highlight = HighlightStyle {
                        background_color: Some(editor_style.status.hint_background),
                        underline: Some(UnderlineStyle {
                            color: Some(editor_style.status.hint),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..Default::default()
                    };
                    let invisible_style = if let Some(style) = style {
                        style.highlight(invisible_highlight)
                    } else {
                        invisible_highlight
                    };

                    return Some(HighlightedChunk {
                        text: prefix,
                        style: Some(invisible_style),
                        is_tab: false,
                        is_inlay,
                        replacement: renderer.clone(),
                    });
                }
            }

            if !text.is_empty() {
                let remainder = text;
                text = "";
                Some(HighlightedChunk {
                    text: remainder,
                    style,
                    is_tab,
                    is_inlay,
                    replacement: renderer.clone(),
                })
            } else {
                None
            }
        })
    }
}

pub(crate) fn diagnostic_style(severity: lsp::DiagnosticSeverity, colors: &StatusColors) -> Hsla {
    match severity {
        lsp::DiagnosticSeverity::ERROR => colors.error,
        lsp::DiagnosticSeverity::WARNING => colors.warning,
        lsp::DiagnosticSeverity::INFORMATION => colors.info,
        lsp::DiagnosticSeverity::HINT => colors.hint,
        _ => colors.ignored,
    }
}

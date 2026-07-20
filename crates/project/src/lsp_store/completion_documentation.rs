use gpui::SharedString;

#[derive(Clone, Debug)]
pub enum CompletionDocumentation {
    /// There is no documentation for this completion.
    Undocumented,
    /// A single line of documentation.
    SingleLine(SharedString),
    /// Multiple lines of plain text documentation.
    MultiLinePlainText(SharedString),
    /// Markdown documentation.
    MultiLineMarkdown(SharedString),
    /// Both single line and multiple lines of plain text documentation.
    SingleLineAndMultiLinePlainText {
        single_line: SharedString,
        plain_text: Option<SharedString>,
    },
}

impl CompletionDocumentation {
    #[cfg(any(test, feature = "test-support"))]
    pub fn text(&self) -> SharedString {
        match self {
            CompletionDocumentation::Undocumented => "".into(),
            CompletionDocumentation::SingleLine(s) => s.clone(),
            CompletionDocumentation::MultiLinePlainText(s) => s.clone(),
            CompletionDocumentation::MultiLineMarkdown(s) => s.clone(),
            CompletionDocumentation::SingleLineAndMultiLinePlainText { single_line, .. } => {
                single_line.clone()
            }
        }
    }
}

impl From<lsp::Documentation> for CompletionDocumentation {
    fn from(docs: lsp::Documentation) -> Self {
        match docs {
            lsp::Documentation::String(text) => {
                if text.lines().count() <= 1 {
                    CompletionDocumentation::SingleLine(text.trim().to_string().into())
                } else {
                    CompletionDocumentation::MultiLinePlainText(text.into())
                }
            }

            lsp::Documentation::MarkupContent(lsp::MarkupContent { kind, value }) => match kind {
                lsp::MarkupKind::PlainText => {
                    if value.lines().count() <= 1 {
                        CompletionDocumentation::SingleLine(value.into())
                    } else {
                        CompletionDocumentation::MultiLinePlainText(value.into())
                    }
                }

                lsp::MarkupKind::Markdown => {
                    CompletionDocumentation::MultiLineMarkdown(value.into())
                }
            },
        }
    }
}

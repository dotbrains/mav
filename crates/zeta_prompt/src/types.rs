use crate::excerpt_ranges::ExcerptRanges;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use strum::{EnumIter, IntoEnumIterator as _, IntoStaticStr};

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct Zeta2PromptInput {
    pub cursor_path: Arc<Path>,
    pub cursor_excerpt: Arc<str>,
    pub cursor_offset_in_excerpt: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt_start_row: Option<u32>,
    pub events: Vec<Arc<Event>>,
    #[serde(default)]
    pub related_files: Option<Vec<RelatedFile>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_buffer_diagnostics: Vec<ActiveBufferDiagnostic>,
    /// These ranges let the server select model-appropriate subsets.
    pub excerpt_ranges: ExcerptRanges,
    /// Byte offset ranges within `cursor_excerpt` for all syntax nodes that
    /// contain `cursor_offset_in_excerpt`, ordered from innermost to outermost.
    /// When present, the server uses these to compute editable/context ranges
    /// instead of `excerpt_ranges`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syntax_ranges: Option<Vec<Range<usize>>>,
    #[serde(default)]
    pub in_open_source_repo: bool,
    #[serde(default)]
    pub can_collect_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FilePosition {
    pub row: u32,
    pub column: u32,
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct Zeta3PromptInput {
    pub cursor_path: Arc<Path>,
    pub cursor_position: FilePosition,
    pub events: Vec<Arc<Event>>,
    pub editable_context: Vec<RelatedFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub syntax_ranges: Vec<Range<usize>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_buffer_diagnostics: Vec<ActiveBufferDiagnostic>,
    #[serde(default)]
    pub in_open_source_repo: bool,
    #[serde(default)]
    pub can_collect_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
}

#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[allow(non_camel_case_types)]
pub enum ZetaFormat {
    V0112MiddleAtEnd,
    V0113Ordered,
    V0114180EditableRegion,
    V0120GitMergeMarkers,
    #[default]
    V0131GitMergeMarkersPrefix,
    V0211Prefill,
    #[serde(alias = "Zeta2")]
    V0211SeedCoder,
    V0331SeedCoderModelPy,
    v0226Hashline,
    V0304VariableEdit,
    V0304SeedNoEdits,
    /// Multi-block marker spans with NO_EDITS sentinel.
    V0306SeedMultiRegions,
    /// Byte-exact marker spans; all intermediate markers emitted; repeated marker means no-edit.
    V0316SeedMultiRegions,
    /// V0316, but marker numbers are relative to the cursor block (e.g. -1, -0, +1).
    V0317SeedMultiRegions,
    /// V0316 with larger block sizes.
    #[serde(alias = "Zeta2.1")]
    V0318SeedMultiRegions,
    /// V0318-style markers over the full available current file excerpt with no related files.
    V0327SingleFile,
    /// V0318-style prompt with buffer diagnostics
    V0420Diagnostics,
    /// V0318-style multi-region format using Qwen FIM tokens and PSM ordering.
    V0608QwenMultiRegions,

    /// V0318-style marker-span output, but with content-hashed marker tags over rendered
    /// related-file context so the model can target jump edits. There is no cursor-centered
    /// editable region for this format.
    V0615HashRegions,
}

impl std::fmt::Display for ZetaFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", <&'static str>::from(self))
    }
}

impl ZetaFormat {
    pub fn parse(format_name: &str) -> Result<Self> {
        let lower = format_name.to_lowercase();

        // Exact case-insensitive match takes priority, bypassing ambiguity checks.
        for variant in ZetaFormat::iter() {
            if <&'static str>::from(&variant).to_lowercase() == lower {
                return Ok(variant);
            }
        }

        let mut results = ZetaFormat::iter().filter(|version| {
            <&'static str>::from(version)
                .to_lowercase()
                .contains(&lower)
        });
        let Some(result) = results.next() else {
            anyhow::bail!(
                "`{format_name}` did not match any of:\n{}",
                Self::options_as_string()
            );
        };
        if results.next().is_some() {
            anyhow::bail!(
                "`{format_name}` matched more than one of:\n{}",
                Self::options_as_string()
            );
        }
        Ok(result)
    }

    pub fn options_as_string() -> String {
        ZetaFormat::iter()
            .map(|format| format!("- {}\n", <&'static str>::from(format)))
            .collect::<Vec<_>>()
            .concat()
    }
}

fn empty_range() -> Range<usize> {
    0..0
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
    BufferChange {
        path: Arc<Path>,
        old_path: Arc<Path>,
        diff: String,
        #[serde(default = "empty_range")]
        old_range: Range<usize>,
        #[serde(default = "empty_range")]
        new_range: Range<usize>,
        predicted: bool,
        in_open_source_repo: bool,
    },
}

impl Event {
    pub fn in_open_source_repo(&self) -> bool {
        match self {
            Event::BufferChange {
                in_open_source_repo,
                ..
            } => *in_open_source_repo,
        }
    }
}

pub fn write_event(prompt: &mut String, event: &Event) {
    fn write_path_as_unix_str(prompt: &mut String, path: &Path) {
        for component in path.components() {
            prompt.push('/');
            write!(prompt, "{}", component.as_os_str().display()).ok();
        }
    }
    match event {
        Event::BufferChange {
            path,
            old_path,
            diff,
            predicted,
            ..
        } => {
            if *predicted {
                prompt.push_str("// User accepted prediction:\n");
            }
            prompt.push_str("--- a");
            write_path_as_unix_str(prompt, old_path.as_ref());
            prompt.push_str("\n+++ b");
            write_path_as_unix_str(prompt, path.as_ref());
            prompt.push('\n');
            prompt.push_str(diff);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct ActiveBufferDiagnostic {
    pub severity: Option<i32>,
    pub message: String,
    pub snippet: String,
    pub snippet_buffer_row_range: Range<u32>,
    pub diagnostic_range_in_snippet: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct RelatedFile {
    pub path: Arc<Path>,
    pub max_row: u32,
    pub excerpts: Vec<RelatedExcerpt>,
    #[serde(default)]
    pub in_open_source_repo: bool,
}

#[derive(Clone, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct RelatedExcerpt {
    pub row_range: Range<u32>,
    pub text: Arc<str>,
    #[serde(default)]
    pub order: usize,
    #[serde(default)]
    pub context_source: ContextSource,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSource {
    #[default]
    Lsp,
    CursorExcerpt,
    CurrentFile,
    EditHistory,
    EditHistoryFile,
    GitLog,
    Bm25,
    OracleFile,
    OracleSnippet,
}

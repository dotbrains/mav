use super::*;

/// Search hits can be painted on either markdown or past-message editors.
#[derive(Clone)]
pub(super) enum MatchTarget {
    Markdown {
        markdown: WeakEntity<Markdown>,
        markdown_match_ix: usize,
    },
    Editor {
        editor: WeakEntity<Editor>,
        anchor_range: Range<Anchor>,
        editor_match_ix: usize,
    },
}

impl MatchTarget {
    pub(super) fn entity_id(&self) -> EntityId {
        match self {
            MatchTarget::Markdown { markdown, .. } => markdown.entity_id(),
            MatchTarget::Editor { editor, .. } => editor.entity_id(),
        }
    }

    /// Index of this hit within its painted entity (markdown- or editor-local).
    pub(super) fn match_ix(&self) -> usize {
        match self {
            MatchTarget::Markdown {
                markdown_match_ix, ..
            } => *markdown_match_ix,
            MatchTarget::Editor {
                editor_match_ix, ..
            } => *editor_match_ix,
        }
    }
}

pub(super) struct ThreadMatch {
    pub(super) entry_ix: usize,
    pub(super) target: MatchTarget,
    pub(super) source_range: Range<usize>,
}

impl ThreadMatch {
    /// Stable identity used to re-locate the active match across a rescan.
    pub(super) fn key(&self) -> MatchKey {
        MatchKey {
            entry_ix: self.entry_ix,
            entity_id: self.target.entity_id(),
            source_range: self.source_range.clone(),
        }
    }
}

#[derive(PartialEq)]
pub(super) struct MatchKey {
    entry_ix: usize,
    entity_id: EntityId,
    source_range: Range<usize>,
}

pub(super) enum SearchTarget {
    Editor {
        entry_ix: usize,
        editor: Entity<Editor>,
        snapshot: MultiBufferSnapshot,
    },
    Markdown {
        entry_ix: usize,
        markdown: Entity<Markdown>,
        source: SharedString,
    },
}

pub(super) enum ScannedTarget {
    Editor {
        entry_ix: usize,
        editor: Entity<Editor>,
        ranges: Vec<Range<usize>>,
        anchor_ranges: Vec<Range<Anchor>>,
    },
    Markdown {
        entry_ix: usize,
        markdown: Entity<Markdown>,
        ranges: Vec<Range<usize>>,
    },
}

pub(super) fn collect_markdowns(
    entry_ix: usize,
    entry: &AgentThreadEntry,
    entry_view_state: &EntryViewState,
    cx: &App,
) -> Vec<Entity<Markdown>> {
    let mut out = Vec::new();
    match entry {
        AgentThreadEntry::UserMessage(_) => {}
        AgentThreadEntry::AssistantMessage(message) => {
            for (chunk_ix, chunk) in message.chunks.iter().enumerate() {
                match chunk {
                    AssistantMessageChunk::Message { block, .. } => {
                        if let Some(md) = block.markdown() {
                            out.push(md.clone());
                        }
                    }
                    AssistantMessageChunk::Thought { block, .. }
                        if entry_view_state
                            .thinking_block_state((entry_ix, chunk_ix), cx)
                            .0 =>
                    {
                        if let Some(md) = block.markdown() {
                            out.push(md.clone());
                        }
                    }
                    AssistantMessageChunk::Thought { .. } => {}
                }
            }
        }
        AgentThreadEntry::ToolCall(tool_call) => {
            out.push(tool_call.label.clone());
            if entry_view_state.is_tool_call_expanded(&tool_call.id) {
                out.extend(
                    tool_call
                        .content
                        .iter()
                        .filter_map(|content| match content {
                            ToolCallContent::ContentBlock(ContentBlock::Markdown { markdown }) => {
                                Some(markdown.clone())
                            }
                            ToolCallContent::ContentBlock(ContentBlock::EmbeddedResource {
                                markdown: Some(markdown),
                                ..
                            }) => Some(markdown.clone()),
                            ToolCallContent::ContentBlock(
                                ContentBlock::Empty
                                | ContentBlock::EmbeddedResource { markdown: None, .. }
                                | ContentBlock::ResourceLink { .. }
                                | ContentBlock::Image { .. },
                            )
                            | ToolCallContent::Diff(_)
                            | ToolCallContent::Terminal(_) => None,
                        }),
                );
            }
        }
        AgentThreadEntry::CompletedPlan(entries) => {
            out.extend(entries.iter().map(|e| e.content.clone()))
        }
        AgentThreadEntry::ContextCompaction(compaction)
            if entry_view_state.is_compaction_expanded(entry_ix) =>
        {
            if let Some(summary) = &compaction.summary {
                out.push(summary.clone());
            }
        }
        AgentThreadEntry::ContextCompaction(_) => {}
    }
    out
}

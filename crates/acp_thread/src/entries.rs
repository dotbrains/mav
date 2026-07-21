use super::*;

#[derive(Debug)]
pub struct UserMessage {
    pub protocol_id: Option<acp::MessageId>,
    pub client_id: Option<ClientUserMessageId>,
    pub is_optimistic: bool,
    pub content: ContentBlock,
    pub chunks: Vec<acp::ContentBlock>,
    pub checkpoint: Option<Checkpoint>,
    pub indented: bool,
}

#[derive(Debug)]
pub struct Checkpoint {
    pub(super) git_checkpoint: GitStoreCheckpoint,
    pub show: bool,
}

impl UserMessage {
    fn to_markdown(&self, cx: &App) -> String {
        let mut markdown = String::new();
        if self
            .checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.show)
        {
            writeln!(markdown, "## User (checkpoint)").unwrap();
        } else {
            writeln!(markdown, "## User").unwrap();
        }
        writeln!(markdown).unwrap();
        writeln!(markdown, "{}", self.content.to_markdown(cx)).unwrap();
        writeln!(markdown).unwrap();
        markdown
    }
}

#[derive(Debug, PartialEq)]
pub struct AssistantMessage {
    pub chunks: Vec<AssistantMessageChunk>,
    pub indented: bool,
    pub is_subagent_output: bool,
}

impl AssistantMessage {
    pub fn to_markdown(&self, cx: &App) -> String {
        format!(
            "## Assistant\n\n{}\n\n",
            self.chunks
                .iter()
                .map(|chunk| chunk.to_markdown(cx))
                .join("\n\n")
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum AssistantMessageChunk {
    Message {
        id: Option<acp::MessageId>,
        block: ContentBlock,
    },
    Thought {
        id: Option<acp::MessageId>,
        block: ContentBlock,
    },
}

impl AssistantMessageChunk {
    pub fn from_str(
        chunk: &str,
        language_registry: &Arc<LanguageRegistry>,
        path_style: PathStyle,
        cx: &mut App,
    ) -> Self {
        Self::Message {
            id: None,
            block: ContentBlock::new(chunk.into(), language_registry, path_style, cx),
        }
    }

    fn to_markdown(&self, cx: &App) -> String {
        match self {
            Self::Message { block, .. } => block.to_markdown(cx).to_string(),
            Self::Thought { block, .. } => {
                format!("<thinking>\n{}\n</thinking>", block.to_markdown(cx))
            }
        }
    }
}

pub(super) fn can_merge_message_chunks(
    existing: Option<&acp::MessageId>,
    incoming: Option<&acp::MessageId>,
) -> bool {
    match (existing, incoming) {
        (Some(existing), Some(incoming)) => existing == incoming,
        _ => true,
    }
}

#[derive(Debug)]
pub enum AgentThreadEntry {
    UserMessage(UserMessage),
    AssistantMessage(AssistantMessage),
    ToolCall(ToolCall),
    CompletedPlan(Vec<PlanEntry>),
    ContextCompaction(ContextCompaction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCompactionId(pub Arc<str>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextCompactionStatus {
    InProgress,
    Completed,
    Canceled,
}

/// A point in the thread where the conversation history was compacted to free
/// up room in the model's context window. The summary can be expanded to inspect
/// what the model retained.
#[derive(Debug)]
pub struct ContextCompaction {
    pub id: ContextCompactionId,
    pub status: ContextCompactionStatus,
    /// The compaction summary, streamed in as the model produces it. This is
    /// `None` for provider-native compaction, which produces no summary to show.
    pub summary: Option<Entity<Markdown>>,
}

impl ContextCompaction {
    pub fn is_in_progress(&self) -> bool {
        self.status == ContextCompactionStatus::InProgress
    }
}

#[derive(Debug)]
pub struct ContextCompactionUpdate {
    pub id: ContextCompactionId,
    pub summary_delta: String,
    pub status: Option<ContextCompactionStatus>,
}

impl AgentThreadEntry {
    pub fn is_indented(&self) -> bool {
        match self {
            Self::UserMessage(message) => message.indented,
            Self::AssistantMessage(message) => message.indented,
            Self::ToolCall(_) => false,
            Self::CompletedPlan(_) => false,
            Self::ContextCompaction(_) => false,
        }
    }

    pub fn to_markdown(&self, cx: &App) -> String {
        match self {
            Self::UserMessage(message) => message.to_markdown(cx),
            Self::AssistantMessage(message) => message.to_markdown(cx),
            Self::ToolCall(tool_call) => tool_call.to_markdown(cx),
            Self::CompletedPlan(entries) => {
                let mut md = String::from("## Plan\n\n");
                for entry in entries {
                    let source = entry.content.read(cx).source().to_string();
                    md.push_str(&format!("- [x] {}\n", source));
                }
                md
            }
            Self::ContextCompaction(_) => "--- Context Compacted ---\n\n".to_string(),
        }
    }

    pub fn user_message(&self) -> Option<&UserMessage> {
        if let AgentThreadEntry::UserMessage(message) = self {
            Some(message)
        } else {
            None
        }
    }

    pub fn diffs(&self) -> impl Iterator<Item = &Entity<Diff>> {
        if let AgentThreadEntry::ToolCall(call) = self {
            itertools::Either::Left(call.diffs())
        } else {
            itertools::Either::Right(std::iter::empty())
        }
    }

    pub fn terminals(&self) -> impl Iterator<Item = &Entity<Terminal>> {
        if let AgentThreadEntry::ToolCall(call) = self {
            itertools::Either::Left(call.terminals())
        } else {
            itertools::Either::Right(std::iter::empty())
        }
    }

    pub fn location(&self, ix: usize) -> Option<(acp::ToolCallLocation, AgentLocation)> {
        if let AgentThreadEntry::ToolCall(ToolCall {
            locations,
            resolved_locations,
            ..
        }) = self
        {
            Some((
                locations.get(ix)?.clone(),
                resolved_locations.get(ix)?.clone()?,
            ))
        } else {
            None
        }
    }
}

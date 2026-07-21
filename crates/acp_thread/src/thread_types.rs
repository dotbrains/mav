use super::*;

#[derive(Debug)]
pub enum ToolCallContent {
    ContentBlock(ContentBlock),
    Diff(Entity<Diff>),
    Terminal(Entity<Terminal>),
}

impl ToolCallContent {
    pub fn from_acp(
        content: acp::ToolCallContent,
        language_registry: Arc<LanguageRegistry>,
        path_style: PathStyle,
        terminals: &HashMap<acp::TerminalId, Entity<Terminal>>,
        cx: &mut App,
    ) -> Result<Option<Self>> {
        match content {
            acp::ToolCallContent::Content(acp::Content { content, .. }) => Ok(Some(
                Self::ContentBlock(ContentBlock::new_tool_call_content(
                    content,
                    &language_registry,
                    path_style,
                    cx,
                )),
            )),
            acp::ToolCallContent::Diff(diff) => Ok(Some(Self::Diff(cx.new(|cx| {
                Diff::finalized(
                    diff.path.to_string_lossy().into_owned(),
                    diff.old_text,
                    diff.new_text,
                    language_registry,
                    cx,
                )
            })))),
            acp::ToolCallContent::Terminal(acp::Terminal { terminal_id, .. }) => terminals
                .get(&terminal_id)
                .cloned()
                .map(|terminal| Some(Self::Terminal(terminal)))
                .ok_or_else(|| anyhow::anyhow!("Terminal with id `{}` not found", terminal_id)),
            _ => Ok(None),
        }
    }

    pub fn update_from_acp(
        &mut self,
        new: acp::ToolCallContent,
        language_registry: Arc<LanguageRegistry>,
        path_style: PathStyle,
        terminals: &HashMap<acp::TerminalId, Entity<Terminal>>,
        cx: &mut App,
    ) -> Result<bool> {
        // Update streaming text in place so the rendered markdown element is
        // reused across snapshots instead of being recreated (which flickers).
        if let (
            Self::ContentBlock(block),
            acp::ToolCallContent::Content(acp::Content { content, .. }),
        ) = (&mut *self, &new)
            && block.update_text_in_place(content, cx)
        {
            return Ok(true);
        }

        let needs_update = match (&self, &new) {
            (Self::Diff(old_diff), acp::ToolCallContent::Diff(new_diff)) => {
                old_diff.read(cx).needs_update(
                    new_diff.old_text.as_deref().unwrap_or(""),
                    &new_diff.new_text,
                    cx,
                )
            }
            _ => true,
        };

        if let Some(update) = Self::from_acp(new, language_registry, path_style, terminals, cx)? {
            if needs_update {
                *self = update;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn to_markdown(&self, cx: &App) -> String {
        match self {
            Self::ContentBlock(content) => content.to_markdown(cx).to_string(),
            Self::Diff(diff) => diff.read(cx).to_markdown(cx),
            Self::Terminal(terminal) => terminal.read(cx).to_markdown(cx),
        }
    }

    pub fn image(&self) -> Option<(&Arc<gpui::Image>, Option<gpui::Size<u32>>)> {
        match self {
            Self::ContentBlock(content) => content.image(),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ToolCallUpdate {
    UpdateFields(acp::ToolCallUpdate),
    UpdateDiff(ToolCallUpdateDiff),
    UpdateTerminal(ToolCallUpdateTerminal),
}

impl ToolCallUpdate {
    pub(super) fn id(&self) -> &acp::ToolCallId {
        match self {
            Self::UpdateFields(update) => &update.tool_call_id,
            Self::UpdateDiff(diff) => &diff.id,
            Self::UpdateTerminal(terminal) => &terminal.id,
        }
    }
}

impl From<acp::ToolCallUpdate> for ToolCallUpdate {
    fn from(update: acp::ToolCallUpdate) -> Self {
        Self::UpdateFields(update)
    }
}

impl From<ToolCallUpdateDiff> for ToolCallUpdate {
    fn from(diff: ToolCallUpdateDiff) -> Self {
        Self::UpdateDiff(diff)
    }
}

#[derive(Debug, PartialEq)]
pub struct ToolCallUpdateDiff {
    pub id: acp::ToolCallId,
    pub diff: Entity<Diff>,
}

impl From<ToolCallUpdateTerminal> for ToolCallUpdate {
    fn from(terminal: ToolCallUpdateTerminal) -> Self {
        Self::UpdateTerminal(terminal)
    }
}

#[derive(Debug, PartialEq)]
pub struct ToolCallUpdateTerminal {
    pub id: acp::ToolCallId,
    pub terminal: Entity<Terminal>,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub entries: Vec<PlanEntry>,
}

#[derive(Debug)]
pub struct PlanStats<'a> {
    pub in_progress_entry: Option<&'a PlanEntry>,
    pub pending: u32,
    pub completed: u32,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn stats(&self) -> PlanStats<'_> {
        let mut stats = PlanStats {
            in_progress_entry: None,
            pending: 0,
            completed: 0,
        };

        for entry in &self.entries {
            match &entry.status {
                acp::PlanEntryStatus::Pending => {
                    stats.pending += 1;
                }
                acp::PlanEntryStatus::InProgress => {
                    stats.in_progress_entry = stats.in_progress_entry.or(Some(entry));
                    stats.pending += 1;
                }
                acp::PlanEntryStatus::Completed => {
                    stats.completed += 1;
                }
                _ => {}
            }
        }

        stats
    }
}

#[derive(Debug)]
pub struct PlanEntry {
    pub content: Entity<Markdown>,
    pub priority: acp::PlanEntryPriority,
    pub status: acp::PlanEntryStatus,
}

impl PlanEntry {
    pub fn from_acp(entry: acp::PlanEntry, cx: &mut App) -> Self {
        Self {
            content: cx.new(|cx| Markdown::new(entry.content.into(), None, None, cx)),
            priority: entry.priority,
            status: entry.status,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub max_tokens: u64,
    pub used_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct SessionCost {
    pub amount: f64,
    pub currency: SharedString,
}

pub const TOKEN_USAGE_WARNING_THRESHOLD: f32 = 0.8;

impl TokenUsage {
    pub fn ratio(&self) -> TokenUsageRatio {
        #[cfg(debug_assertions)]
        let warning_threshold: f32 = std::env::var("MAV_THREAD_WARNING_THRESHOLD")
            .unwrap_or(TOKEN_USAGE_WARNING_THRESHOLD.to_string())
            .parse()
            .unwrap();
        #[cfg(not(debug_assertions))]
        let warning_threshold: f32 = TOKEN_USAGE_WARNING_THRESHOLD;

        // When the maximum is unknown because there is no selected model,
        // avoid showing the token limit warning.
        if self.max_tokens == 0 {
            TokenUsageRatio::Normal
        } else if self.used_tokens >= self.max_tokens {
            TokenUsageRatio::Exceeded
        } else if self.used_tokens as f32 / self.max_tokens as f32 >= warning_threshold {
            TokenUsageRatio::Warning
        } else {
            TokenUsageRatio::Normal
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TokenUsageRatio {
    Normal,
    Warning,
    Exceeded,
}

#[derive(Debug, Clone)]
pub struct RetryStatus {
    pub last_error: SharedString,
    pub attempt: usize,
    pub max_attempts: usize,
    pub started_at: Instant,
    pub duration: Duration,
    pub meta: Option<acp::Meta>,
}

pub const REFUSAL_FALLBACK_MODEL_META_KEY: &str = "refusal_fallback_model";

pub fn meta_with_refusal_fallback(model_name: &str) -> acp::Meta {
    acp::Meta::from_iter([(REFUSAL_FALLBACK_MODEL_META_KEY.into(), model_name.into())])
}

pub fn refusal_fallback_model_from_meta(meta: &Option<acp::Meta>) -> Option<SharedString> {
    meta.as_ref()
        .and_then(|m| m.get(REFUSAL_FALLBACK_MODEL_META_KEY))
        .and_then(|v| v.as_str())
        .map(|s| SharedString::from(s.to_owned()))
}

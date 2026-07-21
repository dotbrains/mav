use super::*;

impl AcpThread {
    pub fn push_context_compaction(
        &mut self,
        compaction: ContextCompaction,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) =
            self.entries
                .iter()
                .enumerate()
                .rev()
                .find_map(|(ix, entry)| match entry {
                    AgentThreadEntry::ContextCompaction(c) if &c.id == &compaction.id => Some(ix),
                    _ => None,
                })
        {
            self.entries[ix] = AgentThreadEntry::ContextCompaction(compaction);
            cx.emit(AcpThreadEvent::EntryUpdated(ix));
        } else {
            self.push_entry(AgentThreadEntry::ContextCompaction(compaction), cx);
        }
    }

    pub fn update_context_compaction(
        &mut self,
        update: ContextCompactionUpdate,
        cx: &mut Context<Self>,
    ) {
        let language_registry = self.project.read(cx).languages().clone();
        let Some((ix, compaction)) =
            self.entries
                .iter_mut()
                .enumerate()
                .rev()
                .find_map(|(ix, entry)| match entry {
                    AgentThreadEntry::ContextCompaction(c) if &c.id == &update.id => Some((ix, c)),
                    _ => None,
                })
        else {
            return;
        };

        if !update.summary_delta.is_empty() {
            if compaction.summary.is_none() {
                compaction.summary = Some(cx.new(|cx| {
                    Markdown::new(
                        update.summary_delta.into(),
                        Some(language_registry),
                        None,
                        cx,
                    )
                }));
            } else if let Some(summary) = compaction.summary.clone() {
                summary.update(cx, |markdown, cx| {
                    markdown.append(&update.summary_delta, cx)
                });
            }
        }

        if let Some(status) = update.status {
            compaction.status = status;
        }

        cx.emit(AcpThreadEvent::EntryUpdated(ix));
    }

    pub fn can_set_title(&mut self, cx: &mut Context<Self>) -> bool {
        self.connection.set_title(&self.session_id, cx).is_some()
    }

    pub fn set_title(&mut self, title: SharedString, cx: &mut Context<Self>) -> Task<Result<()>> {
        let had_provisional = self.provisional_title.take().is_some();
        if self.title.as_ref() != Some(&title) {
            self.title = Some(title.clone());
            cx.emit(AcpThreadEvent::TitleUpdated);
            if let Some(set_title) = self.connection.set_title(&self.session_id, cx) {
                return set_title.run(title, cx);
            }
        } else if had_provisional {
            cx.emit(AcpThreadEvent::TitleUpdated);
        }
        Task::ready(Ok(()))
    }

    /// Sets a provisional display title without propagating back to the
    /// underlying agent connection. This is used for quick preview titles
    /// (e.g. first 20 chars of the user message) that should be shown
    /// immediately but replaced once the LLM generates a proper title via
    /// `set_title`.
    pub fn set_provisional_title(&mut self, title: SharedString, cx: &mut Context<Self>) {
        self.provisional_title = Some(title);
        cx.emit(AcpThreadEvent::TitleUpdated);
    }

    pub fn subagent_spawned(&mut self, session_id: acp::SessionId, cx: &mut Context<Self>) {
        cx.emit(AcpThreadEvent::SubagentSpawned(session_id));
    }

    pub fn update_token_usage(&mut self, usage: Option<TokenUsage>, cx: &mut Context<Self>) {
        if usage.is_none() {
            self.cost = None;
        }
        self.token_usage = usage;
        cx.emit(AcpThreadEvent::TokenUsageUpdated);
    }

    pub fn update_retry_status(&mut self, status: RetryStatus, cx: &mut Context<Self>) {
        cx.emit(AcpThreadEvent::Retry(status));
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn update_plan(&mut self, request: acp::Plan, cx: &mut Context<Self>) {
        let new_entries_len = request.entries.len();
        let mut new_entries = request.entries.into_iter();

        // Reuse existing markdown to prevent flickering
        for (old, new) in self.plan.entries.iter_mut().zip(new_entries.by_ref()) {
            let PlanEntry {
                content,
                priority,
                status,
            } = old;
            content.update(cx, |old, cx| {
                old.replace(new.content, cx);
            });
            *priority = new.priority;
            *status = new.status;
        }
        for new in new_entries {
            self.plan.entries.push(PlanEntry::from_acp(new, cx))
        }
        self.plan.entries.truncate(new_entries_len);

        cx.notify();
    }

    pub fn snapshot_completed_plan(&mut self, cx: &mut Context<Self>) {
        if !self.plan.is_empty() && self.plan.stats().pending == 0 {
            let completed_entries = std::mem::take(&mut self.plan.entries);
            self.push_entry(AgentThreadEntry::CompletedPlan(completed_entries), cx);
        }
    }

    pub(super) fn clear_completed_plan_entries(&mut self, cx: &mut Context<Self>) {
        self.plan
            .entries
            .retain(|entry| !matches!(entry.status, acp::PlanEntryStatus::Completed));
        cx.notify();
    }

    pub fn clear_plan(&mut self, cx: &mut Context<Self>) {
        self.plan.entries.clear();
        cx.notify();
    }
}

use super::*;

#[derive(Debug)]
pub struct ToolCall {
    pub id: acp::ToolCallId,
    pub label: Entity<Markdown>,
    pub kind: acp::ToolKind,
    pub content: Vec<ToolCallContent>,
    pub status: ToolCallStatus,
    pub locations: Vec<acp::ToolCallLocation>,
    pub resolved_locations: Vec<Option<AgentLocation>>,
    pub raw_input: Option<serde_json::Value>,
    pub raw_input_markdown: Option<Entity<Markdown>>,
    pub raw_output: Option<serde_json::Value>,
    pub tool_name: Option<SharedString>,
    pub subagent_session_info: Option<SubagentSessionInfo>,
    pub sandbox_authorization_details: Option<SandboxAuthorizationDetails>,
    pub sandbox_fallback_authorization_details: Option<SandboxFallbackAuthorizationDetails>,
    /// Why this terminal command ran without the OS sandbox even though
    /// sandboxing was active (see [`SANDBOX_NOT_APPLIED_META_KEY`]). `None` when
    /// the command was sandboxed normally (or sandboxing was off).
    pub sandbox_not_applied: Option<SandboxNotAppliedReason>,
}

impl ToolCall {
    pub(super) fn from_acp(
        tool_call: acp::ToolCall,
        status: ToolCallStatus,
        language_registry: Arc<LanguageRegistry>,
        path_style: PathStyle,
        terminals: &HashMap<acp::TerminalId, Entity<Terminal>>,
        cx: &mut App,
    ) -> Result<Self> {
        let title = if tool_call.kind == acp::ToolKind::Execute {
            tool_call.title
        } else if tool_call.kind == acp::ToolKind::Edit {
            MarkdownEscaped(tool_call.title.as_str()).to_string()
        } else if let Some((first_line, _)) = tool_call.title.split_once("\n") {
            first_line.to_owned() + "…"
        } else {
            tool_call.title
        };
        let mut content = Vec::with_capacity(tool_call.content.len());
        for item in tool_call.content {
            if let Some(item) = ToolCallContent::from_acp(
                item,
                language_registry.clone(),
                path_style,
                terminals,
                cx,
            )? {
                content.push(item);
            }
        }

        let raw_input_markdown = tool_call
            .raw_input
            .as_ref()
            .and_then(|input| markdown_for_raw_output(input, &language_registry, cx));

        let tool_name = tool_name_from_meta(&tool_call.meta);

        let subagent_session_info = subagent_session_info_from_meta(&tool_call.meta);
        let sandbox_authorization_details =
            sandbox_authorization_details_from_meta(&tool_call.meta);
        let sandbox_fallback_authorization_details =
            sandbox_fallback_authorization_details_from_meta(&tool_call.meta);
        let sandbox_not_applied = sandbox_not_applied_from_meta(&tool_call.meta);

        let label = if tool_call.kind == acp::ToolKind::Execute {
            cx.new(|cx| Markdown::new_text(title.into(), cx))
        } else {
            cx.new(|cx| Markdown::new(title.into(), Some(language_registry.clone()), None, cx))
        };

        let result = Self {
            id: tool_call.tool_call_id,
            label,
            kind: tool_call.kind,
            content,
            locations: tool_call.locations,
            resolved_locations: Vec::default(),
            status,
            raw_input: tool_call.raw_input,
            raw_input_markdown,
            raw_output: tool_call.raw_output,
            tool_name,
            subagent_session_info,
            sandbox_authorization_details,
            sandbox_fallback_authorization_details,
            sandbox_not_applied,
        };
        Ok(result)
    }

    pub(super) fn update_fields(
        &mut self,
        fields: acp::ToolCallUpdateFields,
        meta: Option<acp::Meta>,
        language_registry: Arc<LanguageRegistry>,
        path_style: PathStyle,
        terminals: &HashMap<acp::TerminalId, Entity<Terminal>>,
        cx: &mut App,
    ) -> Result<()> {
        let acp::ToolCallUpdateFields {
            kind,
            status,
            title,
            content,
            locations,
            raw_input,
            raw_output,
            ..
        } = fields;

        if let Some(kind) = kind {
            self.kind = kind;
        }

        if let Some(status) = status {
            self.update_acp_status(status);
        }

        if let Some(subagent_session_info) = subagent_session_info_from_meta(&meta) {
            self.subagent_session_info = Some(subagent_session_info);
        }
        if let Some(sandbox_authorization_details) = sandbox_authorization_details_from_meta(&meta)
        {
            self.sandbox_authorization_details = Some(sandbox_authorization_details);
        }
        if let Some(sandbox_fallback_authorization_details) =
            sandbox_fallback_authorization_details_from_meta(&meta)
        {
            self.sandbox_fallback_authorization_details =
                Some(sandbox_fallback_authorization_details);
        }
        if let Some(sandbox_not_applied) = sandbox_not_applied_from_meta(&meta) {
            self.sandbox_not_applied = Some(sandbox_not_applied);
        }

        if let Some(title) = title {
            if self.kind == acp::ToolKind::Execute {
                for terminal in self.terminals() {
                    terminal.update(cx, |terminal, cx| {
                        terminal.update_command_label(&title, cx);
                    });
                }
            }
            self.label.update(cx, |label, cx| {
                if self.kind == acp::ToolKind::Execute {
                    label.replace(title, cx);
                } else if self.kind == acp::ToolKind::Edit {
                    label.replace(MarkdownEscaped(&title).to_string(), cx)
                } else if let Some((first_line, _)) = title.split_once("\n") {
                    label.replace(first_line.to_owned() + "…", cx);
                } else {
                    label.replace(title, cx);
                }
            });
        }

        if let Some(content) = content {
            let mut new_content_len = content.len();
            let mut content = content.into_iter();

            // Reuse existing content if we can
            for (old, new) in self.content.iter_mut().zip(content.by_ref()) {
                let valid_content =
                    old.update_from_acp(new, language_registry.clone(), path_style, terminals, cx)?;
                if !valid_content {
                    new_content_len -= 1;
                }
            }
            for new in content {
                if let Some(new) = ToolCallContent::from_acp(
                    new,
                    language_registry.clone(),
                    path_style,
                    terminals,
                    cx,
                )? {
                    self.content.push(new);
                } else {
                    new_content_len -= 1;
                }
            }
            self.content.truncate(new_content_len);
        }

        if let Some(locations) = locations {
            self.locations = locations;
        }

        if let Some(raw_input) = raw_input {
            self.raw_input_markdown = markdown_for_raw_output(&raw_input, &language_registry, cx);
            self.raw_input = Some(raw_input);
        }

        if let Some(raw_output) = raw_output {
            if self.content.is_empty()
                && let Some(markdown) = markdown_for_raw_output(&raw_output, &language_registry, cx)
            {
                self.content
                    .push(ToolCallContent::ContentBlock(ContentBlock::Markdown {
                        markdown,
                    }));
            }
            self.raw_output = Some(raw_output);
        }
        Ok(())
    }

    pub(super) fn update_status(&mut self, status: ToolCallStatus) {
        match status {
            ToolCallStatus::Pending => self.update_acp_status(acp::ToolCallStatus::Pending),
            ToolCallStatus::InProgress => self.update_acp_status(acp::ToolCallStatus::InProgress),
            ToolCallStatus::Completed => self.update_acp_status(acp::ToolCallStatus::Completed),
            ToolCallStatus::Failed => self.update_acp_status(acp::ToolCallStatus::Failed),
            status @ (ToolCallStatus::WaitingForConfirmation { .. }
            | ToolCallStatus::Rejected
            | ToolCallStatus::Canceled) => self.status = status,
        }
    }

    fn update_acp_status(&mut self, status: acp::ToolCallStatus) {
        if let ToolCallStatus::WaitingForConfirmation { current_status, .. } = &mut self.status
            && matches!(
                status,
                acp::ToolCallStatus::Pending | acp::ToolCallStatus::InProgress
            )
        {
            *current_status = status;
        } else {
            self.status = status.into();
        }
    }

    pub fn diffs(&self) -> impl Iterator<Item = &Entity<Diff>> {
        self.content.iter().filter_map(|content| match content {
            ToolCallContent::Diff(diff) => Some(diff),
            ToolCallContent::ContentBlock(_) => None,
            ToolCallContent::Terminal(_) => None,
        })
    }

    pub fn terminals(&self) -> impl Iterator<Item = &Entity<Terminal>> {
        self.content.iter().filter_map(|content| match content {
            ToolCallContent::Terminal(terminal) => Some(terminal),
            ToolCallContent::ContentBlock(_) => None,
            ToolCallContent::Diff(_) => None,
        })
    }

    pub fn is_subagent(&self) -> bool {
        self.tool_name.as_ref().is_some_and(|s| s == "spawn_agent")
            || self.subagent_session_info.is_some()
    }

    pub fn to_markdown(&self, cx: &App) -> String {
        let mut markdown = format!(
            "**Tool Call: {}**\nStatus: {}\n\n",
            self.label.read(cx).source(),
            self.status
        );
        for content in &self.content {
            markdown.push_str(content.to_markdown(cx).as_str());
            markdown.push_str("\n\n");
        }
        markdown
    }

    async fn resolve_location(
        location: acp::ToolCallLocation,
        project: WeakEntity<Project>,
        cx: &mut AsyncApp,
    ) -> Option<ResolvedLocation> {
        let buffer = project
            .update(cx, |project, cx| {
                if let Some(path) = project.project_path_for_absolute_path(&location.path, cx) {
                    Some(project.open_buffer(path, cx))
                } else if is_absolute(
                    location.path.to_string_lossy().as_ref(),
                    project.path_style(cx),
                ) {
                    Some(project.open_local_buffer(&location.path, cx))
                } else {
                    None
                }
            })
            .ok()??;
        let buffer = buffer.await.log_err()?;
        let position = buffer.update(cx, |buffer, _| {
            let snapshot = buffer.snapshot();
            if let Some(row) = location.line {
                let column = snapshot.indent_size_for_line(row).len;
                let point = snapshot.clip_point(Point::new(row, column), Bias::Left);
                snapshot.anchor_before(point)
            } else {
                Anchor::min_for_buffer(snapshot.remote_id())
            }
        });

        Some(ResolvedLocation { buffer, position })
    }

    pub(super) fn resolve_locations(
        &self,
        project: Entity<Project>,
        cx: &mut App,
    ) -> Task<Vec<Option<ResolvedLocation>>> {
        let locations = self.locations.clone();
        project.update(cx, |_, cx| {
            cx.spawn(async move |project, cx| {
                let mut new_locations = Vec::new();
                for location in locations {
                    new_locations.push(Self::resolve_location(location, project.clone(), cx).await);
                }
                new_locations
            })
        })
    }
}

// Separate so we can hold a strong reference to the buffer
// for saving on the thread
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResolvedLocation {
    pub(super) buffer: Entity<Buffer>,
    pub(super) position: Anchor,
}

impl From<&ResolvedLocation> for AgentLocation {
    fn from(value: &ResolvedLocation) -> Self {
        Self {
            buffer: value.buffer.downgrade(),
            position: value.position,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SelectedPermissionParams {
    Terminal { patterns: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct SelectedPermissionOutcome {
    pub option_id: acp::PermissionOptionId,
    pub option_kind: acp::PermissionOptionKind,
    pub params: Option<SelectedPermissionParams>,
}

impl SelectedPermissionOutcome {
    pub fn new(option_id: acp::PermissionOptionId, option_kind: acp::PermissionOptionKind) -> Self {
        Self {
            option_id,
            option_kind,
            params: None,
        }
    }

    pub fn params(mut self, params: Option<SelectedPermissionParams>) -> Self {
        self.params = params;
        self
    }
}

impl From<SelectedPermissionOutcome> for acp::SelectedPermissionOutcome {
    fn from(value: SelectedPermissionOutcome) -> Self {
        Self::new(value.option_id)
    }
}

#[derive(Debug)]
pub enum RequestPermissionOutcome {
    Cancelled,
    Selected(SelectedPermissionOutcome),
}

impl From<RequestPermissionOutcome> for acp::RequestPermissionOutcome {
    fn from(value: RequestPermissionOutcome) -> Self {
        match value {
            RequestPermissionOutcome::Cancelled => Self::Cancelled,
            RequestPermissionOutcome::Selected(outcome) => Self::Selected(outcome.into()),
        }
    }
}

/// What a `WaitingForConfirmation` prompt represents semantically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationKind {
    /// The user is granting or denying permission for the tool call to
    /// proceed. The selected `PermissionOptionKind` determines whether the
    /// tool call transitions to `InProgress` (allow) or `Rejected` (reject).
    /// This is the default for tool authorization prompts.
    PermissionGrant,
    /// The user is choosing between actions for the tool to take next
    /// (for example, "Save" vs "Discard" before editing a dirty buffer).
    /// The tool call always transitions to `InProgress` regardless of the
    /// selected `PermissionOptionKind`; the caller interprets the chosen
    /// `option_id` to decide what to do.
    ActionChoice,
}

#[derive(Debug)]
pub enum ToolCallStatus {
    /// The tool call hasn't started running yet, but we start showing it to
    /// the user.
    Pending,
    /// The tool call is waiting for confirmation from the user.
    WaitingForConfirmation {
        current_status: acp::ToolCallStatus,
        options: PermissionOptions,
        respond_tx: oneshot::Sender<SelectedPermissionOutcome>,
        kind: AuthorizationKind,
    },
    /// The tool call is currently running.
    InProgress,
    /// The tool call completed successfully.
    Completed,
    /// The tool call failed.
    Failed,
    /// The user rejected the tool call.
    Rejected,
    /// The user canceled generation so the tool call was canceled.
    Canceled,
}

impl From<acp::ToolCallStatus> for ToolCallStatus {
    fn from(status: acp::ToolCallStatus) -> Self {
        match status {
            acp::ToolCallStatus::Pending => Self::Pending,
            acp::ToolCallStatus::InProgress => Self::InProgress,
            acp::ToolCallStatus::Completed => Self::Completed,
            acp::ToolCallStatus::Failed => Self::Failed,
            _ => Self::Pending,
        }
    }
}

impl ToolCallStatus {
    pub(super) fn as_acp_status(&self) -> Option<acp::ToolCallStatus> {
        match self {
            ToolCallStatus::Pending => Some(acp::ToolCallStatus::Pending),
            ToolCallStatus::WaitingForConfirmation { current_status, .. } => Some(*current_status),
            ToolCallStatus::InProgress => Some(acp::ToolCallStatus::InProgress),
            ToolCallStatus::Completed => Some(acp::ToolCallStatus::Completed),
            ToolCallStatus::Failed => Some(acp::ToolCallStatus::Failed),
            ToolCallStatus::Rejected | ToolCallStatus::Canceled => None,
        }
    }

    pub(super) fn status_after_permission_grant(status: acp::ToolCallStatus) -> ToolCallStatus {
        match ToolCallStatus::from(status) {
            ToolCallStatus::Pending => ToolCallStatus::InProgress,
            status => status,
        }
    }
}

impl Display for ToolCallStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ToolCallStatus::Pending => "Pending",
                ToolCallStatus::WaitingForConfirmation { .. } => "Waiting for confirmation",
                ToolCallStatus::InProgress => "In Progress",
                ToolCallStatus::Completed => "Completed",
                ToolCallStatus::Failed => "Failed",
                ToolCallStatus::Rejected => "Rejected",
                ToolCallStatus::Canceled => "Canceled",
            }
        )
    }
}

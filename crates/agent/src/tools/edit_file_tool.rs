use super::deserialize_maybe_stringified;
pub(crate) use super::edit_session::PartialEdit;
pub use super::edit_session::{Edit, EditSessionOutput as EditFileToolOutput};
use super::edit_session::{
    EditSession, EditSessionContext, EditSessionMode, EditSessionResult,
    initial_title_from_partial_path, run_session,
};
use crate::{AgentTool, Thread, ToolCallEventStream, ToolInput, ToolInputPayload};
use action_log::ActionLog;
use agent_client_protocol::schema::v1 as acp;
use anyhow::Result;
use futures::FutureExt as _;
use gpui::{App, AsyncApp, Entity, Task, WeakEntity};
use language::LanguageRegistry;
use project::Project;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use ui::SharedString;

const DEFAULT_UI_TEXT: &str = "Editing file";

/// This is a tool for applying edits to an existing file.
///
/// Before using this tool, use the `read_file` tool to understand the file's contents and context.
/// To create a new file or overwrite an existing one with completely new contents, use the `write_file` tool instead.
///
/// The only supported path outside the project is `~/.agents/skills` or a descendant, for global agent skills.
///
/// `read_file` prefixes each line of its output with a line number right-aligned in a
/// 6-character field followed by a single tab, then the line's actual content. When you
/// derive `old_text` or `new_text` from that output, strip this prefix and keep only what
/// comes after the tab, preserving the original indentation (tabs and spaces) exactly.
/// Never include any part of the line number prefix in `old_text` or `new_text`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EditFileToolInput {
    /// The full path of the file to edit in the project.
    ///
    /// WARNING: When specifying which file path need changing, you MUST start each path with one of the project's root directories, unless it's a global agent skill under `~/.agents/skills`.
    ///
    /// The following examples assume we have two root directories in the project:
    /// - /a/b/backend
    /// - /c/d/frontend
    ///
    /// <example>
    /// `backend/src/main.rs`
    ///
    /// Notice how the file path starts with `backend`. Without that, the path would be ambiguous and the call would fail!
    /// </example>
    ///
    /// <example>
    /// `frontend/db.js`
    /// </example>
    ///
    /// <example>
    /// To edit a global agent skill file, you may provide a path under `~/.agents/skills`, such as `~/.agents/skills/my-skill/SKILL.md`.
    /// </example>
    pub path: PathBuf,

    /// List of edit operations to apply sequentially.
    /// Each edit finds `old_text` in the file and replaces it with `new_text`.
    #[serde(deserialize_with = "deserialize_maybe_stringified")]
    pub edits: Vec<Edit>,
}

#[derive(Clone, Default, Debug, Deserialize)]
struct EditFileToolPartialInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_maybe_stringified")]
    edits: Option<Vec<PartialEdit>>,
}

pub struct EditFileTool {
    session_context: Arc<EditSessionContext>,
}

impl EditFileTool {
    pub fn new(
        project: Entity<Project>,
        thread: WeakEntity<Thread>,
        action_log: Entity<ActionLog>,
        language_registry: Arc<LanguageRegistry>,
    ) -> Self {
        Self {
            session_context: Arc::new(EditSessionContext::new(
                project,
                thread,
                action_log,
                language_registry,
            )),
        }
    }

    #[cfg(test)]
    fn authorize(
        &self,
        path: &PathBuf,
        event_stream: &ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<()>> {
        self.session_context
            .authorize(Self::NAME, path, event_stream, cx)
    }

    async fn process_streaming_edits(
        &self,
        input: &mut ToolInput<EditFileToolInput>,
        event_stream: &ToolCallEventStream,
        cx: &mut AsyncApp,
    ) -> EditSessionResult {
        let mut session: Option<EditSession> = None;
        let mut last_path: Option<String> = None;

        loop {
            futures::select! {
                payload = input.next().fuse() => {
                    match payload {
                        Ok(payload) => match payload {
                            ToolInputPayload::Partial(partial) => {
                                if let Ok(parsed) = serde_json::from_value::<EditFileToolPartialInput>(partial) {
                                    let path_complete = parsed.path.is_some()
                                        && parsed.path.as_ref() == last_path.as_ref();

                                    last_path = parsed.path.clone();

                                    if session.is_none()
                                        && path_complete
                                        && let Some(path) = parsed.path.as_ref()
                                    {
                                        match EditSession::new(
                                            PathBuf::from(path),
                                            EditSessionMode::Edit,
                                            Self::NAME,
                                            self.session_context.clone(),
                                            event_stream,
                                            cx,
                                        )
                                        .await
                                        {
                                            Ok(created_session) => session = Some(created_session),
                                            Err(error) => {
                                                log::error!("Failed to create edit session: {}", error);
                                                return EditSessionResult::Failed {
                                                    error,
                                                    session: None,
                                                };
                                            }
                                        }
                                    }

                                    if let Some(current_session) = &mut session
                                        && let Err(error) = current_session.process_edit(parsed.edits.as_deref(), event_stream, cx)
                                    {
                                        log::error!("Failed to process edit: {}", error);
                                        return EditSessionResult::Failed { error, session };
                                    }
                                }
                            }
                            ToolInputPayload::Full(full_input) => {
                                let mut session = if let Some(session) = session {
                                    session
                                } else {
                                    match EditSession::new(
                                        full_input.path.clone(),
                                        EditSessionMode::Edit,
                                        Self::NAME,
                                        self.session_context.clone(),
                                        event_stream,
                                        cx,
                                    )
                                    .await
                                    {
                                        Ok(created_session) => created_session,
                                        Err(error) => {
                                            log::error!("Failed to create edit session: {}", error);
                                            return EditSessionResult::Failed {
                                                error,
                                                session: None,
                                            };
                                        }
                                    }
                                };

                                return match session.finalize_edit(full_input.edits, event_stream, cx).await {
                                    Ok(()) => EditSessionResult::Completed(session),
                                    Err(error) => {
                                        log::error!("Failed to finalize edit: {}", error);
                                        EditSessionResult::Failed {
                                            error,
                                            session: Some(session),
                                        }
                                    }
                                };
                            }
                            ToolInputPayload::InvalidJson { error_message } => {
                                log::error!("Received invalid JSON: {error_message}");
                                return EditSessionResult::Failed {
                                    error: error_message,
                                    session,
                                };
                            }
                        },
                        Err(error) => {
                            return EditSessionResult::Failed {
                                error: error.to_string(),
                                session,
                            };
                        }
                    }
                }
                _ = event_stream.cancelled_by_user().fuse() => {
                    return EditSessionResult::Failed {
                        error: "Edit cancelled by user".to_string(),
                        session,
                    };
                }
            }
        }
    }
}

impl AgentTool for EditFileTool {
    type Input = EditFileToolInput;
    type Output = EditFileToolOutput;

    const NAME: &'static str = "edit_file";

    fn supports_input_streaming() -> bool {
        true
    }

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Edit
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        cx: &mut App,
    ) -> SharedString {
        match input {
            Ok(input) => {
                self.session_context
                    .initial_title_from_path(&input.path, DEFAULT_UI_TEXT, cx)
            }
            Err(raw_input) => initial_title_from_partial_path::<EditFileToolPartialInput>(
                &self.session_context,
                raw_input,
                |partial| partial.path.clone(),
                DEFAULT_UI_TEXT,
                cx,
            ),
        }
    }

    fn run(
        self: Arc<Self>,
        mut input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        cx.spawn(async move |cx: &mut AsyncApp| {
            run_session(
                self.process_streaming_edits(&mut input, &event_stream, cx)
                    .await,
                &event_stream,
                cx,
            )
            .await
        })
    }

    fn replay(
        &self,
        _input: Self::Input,
        output: Self::Output,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Result<()> {
        self.session_context.replay_output(output, event_stream, cx)
    }
}

#[cfg(test)]
#[cfg(test)]
#[path = "edit_file_tool/tests.rs"]
mod tests;

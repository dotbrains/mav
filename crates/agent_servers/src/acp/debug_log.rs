use acp_thread::LoadError;
use agent_client_protocol::schema::v1 as acp;
use anyhow::Result;
use gpui::SharedString;
use std::collections::VecDeque;
use std::process::ExitStatus;
use std::sync::{Arc, Mutex};
use util::ResultExt as _;

const MAX_DEBUG_BACKLOG_MESSAGES: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpDebugMessageDirection {
    Incoming,
    Outgoing,
    Stderr,
}

#[derive(Clone)]
pub enum AcpDebugMessageContent {
    Request {
        id: acp::RequestId,
        method: Arc<str>,
        params: Option<serde_json::Value>,
    },
    Response {
        id: acp::RequestId,
        result: Result<Option<serde_json::Value>, acp::Error>,
    },
    Notification {
        method: Arc<str>,
        params: Option<serde_json::Value>,
    },
    Stderr {
        line: Arc<str>,
    },
}

#[derive(Clone)]
pub struct AcpDebugMessage {
    pub direction: AcpDebugMessageDirection,
    pub message: AcpDebugMessageContent,
}

impl AcpDebugMessage {
    fn parse(direction: AcpDebugMessageDirection, line: &str) -> Option<Self> {
        if direction == AcpDebugMessageDirection::Stderr {
            return Some(Self {
                direction,
                message: AcpDebugMessageContent::Stderr {
                    line: Arc::from(line),
                },
            });
        }

        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let object = value.as_object()?;

        let parsed_id = object
            .get("id")
            .map(|raw| serde_json::from_value::<acp::RequestId>(raw.clone()));

        let message = if let Some(method) = object.get("method").and_then(|method| method.as_str())
        {
            match parsed_id {
                Some(Ok(id)) => AcpDebugMessageContent::Request {
                    id,
                    method: method.into(),
                    params: object.get("params").cloned(),
                },
                Some(Err(err)) => {
                    log::warn!("Skipping JSON-RPC message with unparsable id: {err}");
                    return None;
                }
                None => AcpDebugMessageContent::Notification {
                    method: method.into(),
                    params: object.get("params").cloned(),
                },
            }
        } else if let Some(parsed_id) = parsed_id {
            let id = match parsed_id {
                Ok(id) => id,
                Err(err) => {
                    log::warn!("Skipping JSON-RPC response with unparsable id: {err}");
                    return None;
                }
            };

            if let Some(error) = object.get("error") {
                let acp_error =
                    serde_json::from_value::<acp::Error>(error.clone()).unwrap_or_else(|err| {
                        log::warn!("Failed to deserialize ACP error: {err}");
                        acp::Error::internal_error().data(error.to_string())
                    });

                AcpDebugMessageContent::Response {
                    id,
                    result: Err(acp_error),
                }
            } else {
                AcpDebugMessageContent::Response {
                    id,
                    result: Ok(object.get("result").cloned()),
                }
            }
        } else {
            return None;
        };

        Some(Self { direction, message })
    }
}

#[derive(Default)]
struct AcpDebugLogState {
    messages: VecDeque<AcpDebugMessage>,
    subscribers: Vec<async_channel::Sender<AcpDebugMessage>>,
}

#[derive(Clone, Default)]
pub(super) struct AcpDebugLog {
    state: Arc<Mutex<AcpDebugLogState>>,
}

impl AcpDebugLog {
    pub(super) fn subscribe(
        &self,
    ) -> (
        Vec<AcpDebugMessage>,
        async_channel::Receiver<AcpDebugMessage>,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let backlog = state.messages.iter().cloned().collect();
        let (sender, receiver) = async_channel::unbounded();
        state.subscribers.push(sender);
        (backlog, receiver)
    }

    pub(super) fn record_line(&self, direction: AcpDebugMessageDirection, line: &str) {
        let Some(message) = AcpDebugMessage::parse(direction, line) else {
            return;
        };
        self.record_message(message);
    }

    pub(super) fn record_message(&self, message: AcpDebugMessage) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if state.messages.len() == MAX_DEBUG_BACKLOG_MESSAGES {
            state.messages.pop_front();
        }
        state.messages.push_back(message.clone());

        state.subscribers.retain(|sender| !sender.is_closed());
        for sender in &state.subscribers {
            sender.try_send(message.clone()).log_err();
        }
    }

    pub(super) fn trailing_stderr(&self) -> Option<String> {
        let state = self.state.lock().ok()?;
        let mut lines = state
            .messages
            .iter()
            .rev()
            .take_while(|message| matches!(&message.message, AcpDebugMessageContent::Stderr { .. }))
            .filter_map(|message| match &message.message {
                AcpDebugMessageContent::Stderr { line } if !line.is_empty() => Some(line.as_ref()),
                _ => None,
            })
            .collect::<Vec<_>>();

        if lines.is_empty() {
            return None;
        }

        lines.reverse();
        Some(lines.join("\n"))
    }
}

pub(super) fn exited_load_error_with_stderr(
    status: ExitStatus,
    debug_log: &AcpDebugLog,
) -> LoadError {
    LoadError::Exited {
        status,
        stderr: debug_log.trailing_stderr().map(SharedString::from),
    }
}

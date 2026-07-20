use crate::lsp_store::NEXT_PROMPT_REQUEST_ID;
use client::proto;
use gpui::PromptLevel;
use lsp::{MessageActionItem, MessageType};
use std::sync::atomic;

/// A prompt requested by LSP server.
#[derive(Clone, Debug)]
pub struct LanguageServerPromptRequest {
    pub id: usize,
    pub level: PromptLevel,
    pub message: String,
    pub actions: Vec<MessageActionItem>,
    pub lsp_name: String,
    pub(crate) response_channel: async_channel::Sender<MessageActionItem>,
}

impl LanguageServerPromptRequest {
    pub fn new(
        level: PromptLevel,
        message: String,
        actions: Vec<MessageActionItem>,
        lsp_name: String,
        response_channel: async_channel::Sender<MessageActionItem>,
    ) -> Self {
        let id = NEXT_PROMPT_REQUEST_ID.fetch_add(1, atomic::Ordering::AcqRel);
        LanguageServerPromptRequest {
            id,
            level,
            message,
            actions,
            lsp_name,
            response_channel,
        }
    }

    pub async fn respond(self, index: usize) -> Option<()> {
        if let Some(response) = self.actions.into_iter().nth(index) {
            self.response_channel.send(response).await.ok()
        } else {
            None
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(
        level: PromptLevel,
        message: String,
        actions: Vec<MessageActionItem>,
        lsp_name: String,
    ) -> Self {
        let (tx, _rx) = async_channel::unbounded();
        LanguageServerPromptRequest::new(level, message, actions, lsp_name, tx)
    }
}

impl PartialEq for LanguageServerPromptRequest {
    fn eq(&self, other: &Self) -> bool {
        self.message == other.message && self.actions == other.actions
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LanguageServerLogType {
    Log(MessageType),
    Trace { verbose_info: Option<String> },
    Rpc { received: bool },
}

impl LanguageServerLogType {
    pub fn to_proto(&self) -> proto::language_server_log::LogType {
        match self {
            Self::Log(log_type) => {
                use proto::log_message::LogLevel;
                let level = match *log_type {
                    MessageType::ERROR => LogLevel::Error,
                    MessageType::WARNING => LogLevel::Warning,
                    MessageType::INFO => LogLevel::Info,
                    MessageType::LOG => LogLevel::Log,
                    other => {
                        log::warn!("Unknown lsp log message type: {other:?}");
                        LogLevel::Log
                    }
                };
                proto::language_server_log::LogType::Log(proto::LogMessage {
                    level: level as i32,
                })
            }
            Self::Trace { verbose_info } => {
                proto::language_server_log::LogType::Trace(proto::TraceMessage {
                    verbose_info: verbose_info.to_owned(),
                })
            }
            Self::Rpc { received } => {
                let kind = if *received {
                    proto::rpc_message::Kind::Received
                } else {
                    proto::rpc_message::Kind::Sent
                };
                let kind = kind as i32;
                proto::language_server_log::LogType::Rpc(proto::RpcMessage { kind })
            }
        }
    }

    pub fn from_proto(log_type: proto::language_server_log::LogType) -> Self {
        use proto::log_message::LogLevel;
        use proto::rpc_message;
        match log_type {
            proto::language_server_log::LogType::Log(message_type) => Self::Log(
                match LogLevel::from_i32(message_type.level).unwrap_or(LogLevel::Log) {
                    LogLevel::Error => MessageType::ERROR,
                    LogLevel::Warning => MessageType::WARNING,
                    LogLevel::Info => MessageType::INFO,
                    LogLevel::Log => MessageType::LOG,
                },
            ),
            proto::language_server_log::LogType::Trace(trace_message) => Self::Trace {
                verbose_info: trace_message.verbose_info,
            },
            proto::language_server_log::LogType::Rpc(message) => Self::Rpc {
                received: match rpc_message::Kind::from_i32(message.kind)
                    .unwrap_or(rpc_message::Kind::Received)
                {
                    rpc_message::Kind::Received => true,
                    rpc_message::Kind::Sent => false,
                },
            },
        }
    }
}

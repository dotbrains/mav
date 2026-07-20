use client::proto;
use gpui::SharedString;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ProgressToken {
    Number(i32),
    String(SharedString),
}

impl std::fmt::Display for ProgressToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(number) => write!(f, "{number}"),
            Self::String(string) => write!(f, "{string}"),
        }
    }
}

impl ProgressToken {
    pub(crate) fn from_lsp(value: lsp::NumberOrString) -> Self {
        match value {
            lsp::NumberOrString::Number(number) => Self::Number(number),
            lsp::NumberOrString::String(string) => Self::String(SharedString::new(string)),
        }
    }

    pub(crate) fn to_lsp(&self) -> lsp::NumberOrString {
        match self {
            Self::Number(number) => lsp::NumberOrString::Number(*number),
            Self::String(string) => lsp::NumberOrString::String(string.to_string()),
        }
    }

    pub(crate) fn from_proto(value: proto::ProgressToken) -> Option<Self> {
        Some(match value.value? {
            proto::progress_token::Value::Number(number) => Self::Number(number),
            proto::progress_token::Value::String(string) => Self::String(SharedString::new(string)),
        })
    }

    pub(crate) fn to_proto(&self) -> proto::ProgressToken {
        proto::ProgressToken {
            value: Some(match self {
                Self::Number(number) => proto::progress_token::Value::Number(*number),
                Self::String(string) => proto::progress_token::Value::String(string.to_string()),
            }),
        }
    }
}

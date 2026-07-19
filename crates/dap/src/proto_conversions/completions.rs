use super::{ProtoConversion, proto};
use client::proto::DapEvaluateContext;

impl ProtoConversion for dap_types::CompletionItem {
    type ProtoType = proto::DapCompletionItem;
    type Output = Self;

    fn to_proto(self) -> Self::ProtoType {
        Self::ProtoType {
            label: self.label.clone(),
            text: self.text.clone(),
            detail: self.detail.clone(),
            typ: self
                .type_
                .map(ProtoConversion::to_proto)
                .map(|typ| typ.into()),
            start: self.start,
            length: self.length,
            selection_start: self.selection_start,
            selection_length: self.selection_length,
            sort_text: self.sort_text,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        let typ = payload.typ(); // todo(debugger): This might be a potential issue/bug because it defaults to a type when it's None

        Self {
            label: payload.label,
            detail: payload.detail,
            sort_text: payload.sort_text,
            text: payload.text.clone(),
            type_: Some(dap_types::CompletionItemType::from_proto(typ)),
            start: payload.start,
            length: payload.length,
            selection_start: payload.selection_start,
            selection_length: payload.selection_length,
        }
    }
}

impl ProtoConversion for dap_types::EvaluateArgumentsContext {
    type ProtoType = DapEvaluateContext;
    type Output = Self;

    fn to_proto(self) -> Self::ProtoType {
        match self {
            Self::Variables => Self::ProtoType::EvaluateVariables,
            Self::Watch => Self::ProtoType::Watch,
            Self::Hover => Self::ProtoType::Hover,
            Self::Repl => Self::ProtoType::Repl,
            Self::Clipboard => Self::ProtoType::Clipboard,
            Self::Unknown => Self::ProtoType::EvaluateUnknown,
            _ => Self::ProtoType::EvaluateUnknown,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            Self::ProtoType::EvaluateVariables => Self::Variables,
            Self::ProtoType::Watch => Self::Watch,
            Self::ProtoType::Hover => Self::Hover,
            Self::ProtoType::Repl => Self::Repl,
            Self::ProtoType::Clipboard => Self::Clipboard,
            Self::ProtoType::EvaluateUnknown => Self::Unknown,
        }
    }
}

impl ProtoConversion for dap_types::CompletionItemType {
    type ProtoType = proto::DapCompletionItemType;
    type Output = Self;

    fn to_proto(self) -> Self::ProtoType {
        match self {
            Self::Class => Self::ProtoType::Class,
            Self::Color => Self::ProtoType::Color,
            Self::Constructor => Self::ProtoType::Constructor,
            Self::Customcolor => Self::ProtoType::Customcolor,
            Self::Enum => Self::ProtoType::Enum,
            Self::Field => Self::ProtoType::Field,
            Self::File => Self::ProtoType::CompletionItemFile,
            Self::Function => Self::ProtoType::Function,
            Self::Interface => Self::ProtoType::Interface,
            Self::Keyword => Self::ProtoType::Keyword,
            Self::Method => Self::ProtoType::Method,
            Self::Module => Self::ProtoType::Module,
            Self::Property => Self::ProtoType::Property,
            Self::Reference => Self::ProtoType::Reference,
            Self::Snippet => Self::ProtoType::Snippet,
            Self::Text => Self::ProtoType::Text,
            Self::Unit => Self::ProtoType::Unit,
            Self::Value => Self::ProtoType::Value,
            Self::Variable => Self::ProtoType::Variable,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        match payload {
            Self::ProtoType::Class => Self::Class,
            Self::ProtoType::Color => Self::Color,
            Self::ProtoType::CompletionItemFile => Self::File,
            Self::ProtoType::Constructor => Self::Constructor,
            Self::ProtoType::Customcolor => Self::Customcolor,
            Self::ProtoType::Enum => Self::Enum,
            Self::ProtoType::Field => Self::Field,
            Self::ProtoType::Function => Self::Function,
            Self::ProtoType::Interface => Self::Interface,
            Self::ProtoType::Keyword => Self::Keyword,
            Self::ProtoType::Method => Self::Method,
            Self::ProtoType::Module => Self::Module,
            Self::ProtoType::Property => Self::Property,
            Self::ProtoType::Reference => Self::Reference,
            Self::ProtoType::Snippet => Self::Snippet,
            Self::ProtoType::Text => Self::Text,
            Self::ProtoType::Unit => Self::Unit,
            Self::ProtoType::Value => Self::Value,
            Self::ProtoType::Variable => Self::Variable,
        }
    }
}

impl ProtoConversion for dap_types::Thread {
    type ProtoType = proto::DapThread;
    type Output = Self;

    fn to_proto(self) -> Self::ProtoType {
        proto::DapThread {
            id: self.id,
            name: self.name,
        }
    }

    fn from_proto(payload: Self::ProtoType) -> Self {
        Self {
            id: payload.id,
            name: payload.name,
        }
    }
}

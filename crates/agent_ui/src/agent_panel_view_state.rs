use gpui::Entity;
use terminal_view::TerminalView;

use super::agent_panel_terminal::TerminalId;
use crate::{ConversationView, agent_configuration::AgentConfiguration};

pub(crate) struct AgentThread {
    pub(crate) conversation_view: Entity<ConversationView>,
}

pub(crate) enum BaseView {
    Uninitialized,
    AgentThread {
        conversation_view: Entity<ConversationView>,
    },
    Terminal {
        terminal_id: TerminalId,
    },
}

impl From<AgentThread> for BaseView {
    fn from(thread: AgentThread) -> Self {
        BaseView::AgentThread {
            conversation_view: thread.conversation_view,
        }
    }
}

pub(crate) enum OverlayView {
    Configuration,
}

pub(crate) enum VisibleSurface<'a> {
    Uninitialized,
    AgentThread(&'a Entity<ConversationView>),
    Terminal(&'a Entity<TerminalView>),
    Configuration(Option<&'a Entity<AgentConfiguration>>),
}

pub(crate) enum WhichFontSize {
    AgentFont,
    None,
}

impl BaseView {
    pub(crate) fn which_font_size_used(&self) -> WhichFontSize {
        match self {
            BaseView::AgentThread { .. } => WhichFontSize::AgentFont,
            BaseView::Terminal { .. } | BaseView::Uninitialized => WhichFontSize::None,
        }
    }
}

impl OverlayView {
    pub(crate) fn which_font_size_used(&self) -> WhichFontSize {
        match self {
            OverlayView::Configuration => WhichFontSize::None,
        }
    }
}

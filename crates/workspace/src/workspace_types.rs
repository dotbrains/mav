use std::path::PathBuf;

use anyhow::{Context as _, Result};
use client::proto::{self, PeerId};
use gpui::SharedString;

use crate::{dock::DockPosition, persistence::model::DockStructure};

/// Tracks worktree creation progress for the workspace.
/// Read by the title bar to show a loading indicator on the worktree button.
#[derive(Default)]
pub struct ActiveWorktreeCreation {
    pub label: Option<SharedString>,
    pub is_switch: bool,
}

/// Captured workspace state used when switching between worktrees.
/// Stores the layout and open files so they can be restored in the new workspace.
pub struct PreviousWorkspaceState {
    pub dock_structure: DockStructure,
    pub open_file_paths: Vec<PathBuf>,
    pub active_file_path: Option<PathBuf>,
    pub focused_dock: Option<DockPosition>,
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub enum CollaboratorId {
    PeerId(PeerId),
    Agent,
}

impl From<PeerId> for CollaboratorId {
    fn from(peer_id: PeerId) -> Self {
        CollaboratorId::PeerId(peer_id)
    }
}

impl From<&PeerId> for CollaboratorId {
    fn from(peer_id: &PeerId) -> Self {
        CollaboratorId::PeerId(*peer_id)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ViewId {
    pub creator: CollaboratorId,
    pub id: u64,
}

impl ViewId {
    pub(crate) fn from_proto(message: proto::ViewId) -> Result<Self> {
        Ok(Self {
            creator: message
                .creator
                .map(CollaboratorId::PeerId)
                .context("creator is missing")?,
            id: message.id,
        })
    }

    pub(crate) fn to_proto(self) -> Option<proto::ViewId> {
        if let CollaboratorId::PeerId(peer_id) = self.creator {
            Some(proto::ViewId {
                creator: Some(peer_id),
                id: self.id,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoWatch {
    Off,
    Active { watched_peer: Option<PeerId> },
    Paused,
}

impl AutoWatch {
    pub fn enabled(&self) -> bool {
        matches!(self, AutoWatch::Active { .. } | AutoWatch::Paused)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenMode {
    /// Open the workspace in a new window.
    NewWindow,
    /// Add to the window's multi workspace without activating it (used during deserialization).
    Add,
    /// Add to the window's multi workspace and activate it.
    #[default]
    Activate,
}

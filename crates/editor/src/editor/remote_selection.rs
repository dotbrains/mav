use super::*;

#[derive(Debug)]
pub struct RemoteSelection {
    pub replica_id: ReplicaId,
    pub selection: Selection<Anchor>,
    pub cursor_shape: CursorShape,
    pub collaborator_id: CollaboratorId,
    pub line_mode: bool,
    pub user_name: Option<SharedString>,
    pub color: PlayerColor,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct HoveredCursor {
    pub(crate) replica_id: ReplicaId,
    pub(crate) selection_id: usize,
}

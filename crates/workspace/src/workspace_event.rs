use super::{ItemHandle, Pane, SaveIntent, WeakItemHandle, Workspace};
use gpui::{AnyView, Entity, EntityId, WeakEntity};
use std::borrow::Cow;

pub enum Event {
    PaneAdded(Entity<Pane>),
    PaneRemoved,
    ItemAdded {
        item: Box<dyn ItemHandle>,
    },
    ActiveItemChanged,
    ItemRemoved {
        item_id: EntityId,
    },
    UserSavedItem {
        pane: WeakEntity<Pane>,
        item: Box<dyn WeakItemHandle>,
        save_intent: SaveIntent,
    },
    ContactRequestedJoin(u64),
    WorkspaceCreated(WeakEntity<Workspace>),
    OpenBundledFile {
        text: Cow<'static, str>,
        title: &'static str,
        language: &'static str,
    },
    ZoomChanged,
    ModalOpened,
    Activate,
    PanelAdded(AnyView),
    WorktreeCreationChanged,
}

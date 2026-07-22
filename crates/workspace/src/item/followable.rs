use super::*;

#[derive(Debug)]
pub enum FollowEvent {
    Unfollow,
}

pub enum Dedup {
    KeepExisting,
    ReplaceExisting,
}

pub trait FollowableItem: Item {
    fn remote_id(&self) -> Option<ViewId>;
    fn to_state_proto(&self, window: &mut Window, cx: &mut App) -> Option<proto::view::Variant>;
    fn from_state_proto(
        project: Entity<Workspace>,
        id: ViewId,
        state: &mut Option<proto::view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Task<Result<Entity<Self>>>>;
    fn to_follow_event(event: &Self::Event) -> Option<FollowEvent>;
    fn add_event_to_update_proto(
        &self,
        event: &Self::Event,
        update: &mut Option<proto::update_view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool;
    fn apply_update_proto(
        &mut self,
        project: &Entity<Project>,
        message: proto::update_view::Variant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>>;
    fn is_project_item(&self, window: &Window, cx: &App) -> bool;
    fn set_leader_id(
        &mut self,
        leader_peer_id: Option<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn dedup(&self, existing: &Self, window: &Window, cx: &App) -> Option<Dedup>;
    fn update_agent_location(
        &mut self,
        _location: language::Anchor,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

pub trait FollowableItemHandle: ItemHandle {
    fn remote_id(&self, client: &Arc<Client>, window: &mut Window, cx: &mut App) -> Option<ViewId>;
    fn downgrade(&self) -> Box<dyn WeakFollowableItemHandle>;
    fn set_leader_id(
        &self,
        leader_peer_id: Option<CollaboratorId>,
        window: &mut Window,
        cx: &mut App,
    );
    fn to_state_proto(&self, window: &mut Window, cx: &mut App) -> Option<proto::view::Variant>;
    fn add_event_to_update_proto(
        &self,
        event: &dyn Any,
        update: &mut Option<proto::update_view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool;
    fn to_follow_event(&self, event: &dyn Any) -> Option<FollowEvent>;
    fn apply_update_proto(
        &self,
        project: &Entity<Project>,
        message: proto::update_view::Variant,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>>;
    fn is_project_item(&self, window: &mut Window, cx: &mut App) -> bool;
    fn dedup(
        &self,
        existing: &dyn FollowableItemHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Dedup>;
    fn update_agent_location(&self, location: language::Anchor, window: &mut Window, cx: &mut App);
}

impl<T: FollowableItem> FollowableItemHandle for Entity<T> {
    fn remote_id(&self, client: &Arc<Client>, _: &mut Window, cx: &mut App) -> Option<ViewId> {
        self.read(cx).remote_id().or_else(|| {
            client.peer_id().map(|creator| ViewId {
                creator: CollaboratorId::PeerId(creator),
                id: self.item_id().as_u64(),
            })
        })
    }

    fn downgrade(&self) -> Box<dyn WeakFollowableItemHandle> {
        Box::new(self.downgrade())
    }

    fn set_leader_id(&self, leader_id: Option<CollaboratorId>, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_leader_id(leader_id, window, cx))
    }

    fn to_state_proto(&self, window: &mut Window, cx: &mut App) -> Option<proto::view::Variant> {
        self.update(cx, |this, cx| this.to_state_proto(window, cx))
    }

    fn add_event_to_update_proto(
        &self,
        event: &dyn Any,
        update: &mut Option<proto::update_view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if let Some(event) = event.downcast_ref() {
            self.update(cx, |this, cx| {
                this.add_event_to_update_proto(event, update, window, cx)
            })
        } else {
            false
        }
    }

    fn to_follow_event(&self, event: &dyn Any) -> Option<FollowEvent> {
        T::to_follow_event(event.downcast_ref()?)
    }

    fn apply_update_proto(
        &self,
        project: &Entity<Project>,
        message: proto::update_view::Variant,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        self.update(cx, |this, cx| {
            this.apply_update_proto(project, message, window, cx)
        })
    }

    fn is_project_item(&self, window: &mut Window, cx: &mut App) -> bool {
        self.read(cx).is_project_item(window, cx)
    }

    fn dedup(
        &self,
        existing: &dyn FollowableItemHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Dedup> {
        let existing = existing.to_any_view().downcast::<T>().ok()?;
        self.read(cx).dedup(existing.read(cx), window, cx)
    }

    fn update_agent_location(&self, location: language::Anchor, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.update_agent_location(location, window, cx)
        })
    }
}

pub trait WeakFollowableItemHandle: Send + Sync {
    fn upgrade(&self) -> Option<Box<dyn FollowableItemHandle>>;
}

impl<T: FollowableItem> WeakFollowableItemHandle for WeakEntity<T> {
    fn upgrade(&self) -> Option<Box<dyn FollowableItemHandle>> {
        Some(Box::new(self.upgrade()?))
    }
}

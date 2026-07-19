use anyhow::Result;
use collections::{HashMap, TypeIdHashMap};
use futures::future::LocalBoxFuture;
use gpui::{AnyEntity, AnyWeakEntity, AsyncApp};
use proto::AnyTypedEnvelope;
use std::{any::TypeId, sync::Arc};

use super::AnyProtoClient;

#[derive(Default)]
pub struct ProtoMessageHandlerSet {
    pub entity_types_by_message_type: TypeIdHashMap<TypeId>,
    pub entities_by_type_and_remote_id: HashMap<(TypeId, u64), EntityMessageSubscriber>,
    pub entity_id_extractors: TypeIdHashMap<fn(&dyn AnyTypedEnvelope) -> u64>,
    pub entities_by_message_type: TypeIdHashMap<AnyWeakEntity>,
    pub message_handlers: TypeIdHashMap<ProtoMessageHandler>,
}

pub type ProtoMessageHandler = Arc<
    dyn Send
        + Sync
        + Fn(
            AnyEntity,
            Box<dyn AnyTypedEnvelope>,
            AnyProtoClient,
            AsyncApp,
        ) -> LocalBoxFuture<'static, Result<()>>,
>;

impl ProtoMessageHandlerSet {
    pub fn clear(&mut self) {
        self.message_handlers.clear();
        self.entities_by_message_type.clear();
        self.entities_by_type_and_remote_id.clear();
        self.entity_id_extractors.clear();
    }

    pub(super) fn add_message_handler(
        &mut self,
        message_type_id: TypeId,
        entity: gpui::AnyWeakEntity,
        handler: ProtoMessageHandler,
    ) {
        self.entities_by_message_type
            .insert(message_type_id, entity);
        let prev_handler = self.message_handlers.insert(message_type_id, handler);
        if prev_handler.is_some() {
            panic!("registered handler for the same message twice");
        }
    }

    pub(super) fn add_entity_message_handler(
        &mut self,
        message_type_id: TypeId,
        entity_type_id: TypeId,
        entity_id_extractor: fn(&dyn AnyTypedEnvelope) -> u64,
        handler: ProtoMessageHandler,
    ) {
        self.entity_id_extractors
            .entry(message_type_id)
            .or_insert(entity_id_extractor);
        self.entity_types_by_message_type
            .insert(message_type_id, entity_type_id);
        let prev_handler = self.message_handlers.insert(message_type_id, handler);
        if prev_handler.is_some() {
            panic!("registered handler for the same message twice");
        }
    }

    pub fn handle_message(
        this: &parking_lot::Mutex<Self>,
        message: Box<dyn AnyTypedEnvelope>,
        client: AnyProtoClient,
        cx: AsyncApp,
    ) -> Option<LocalBoxFuture<'static, Result<()>>> {
        let payload_type_id = message.payload_type_id();
        let mut this = this.lock();
        let handler = this.message_handlers.get(&payload_type_id)?.clone();
        let entity = if let Some(entity) = this.entities_by_message_type.get(&payload_type_id) {
            entity.upgrade()?
        } else {
            let extract_entity_id = *this.entity_id_extractors.get(&payload_type_id)?;
            let entity_type_id = *this.entity_types_by_message_type.get(&payload_type_id)?;
            let entity_id = (extract_entity_id)(message.as_ref());
            match this
                .entities_by_type_and_remote_id
                .get_mut(&(entity_type_id, entity_id))?
            {
                EntityMessageSubscriber::Pending(pending) => {
                    pending.push(message);
                    return None;
                }
                EntityMessageSubscriber::Entity { handle } => handle.upgrade()?,
            }
        };
        drop(this);
        Some(handler(entity, message, client, cx))
    }
}

pub enum EntityMessageSubscriber {
    Entity { handle: AnyWeakEntity },
    Pending(Vec<Box<dyn AnyTypedEnvelope>>),
}

impl std::fmt::Debug for EntityMessageSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityMessageSubscriber::Entity { handle } => f
                .debug_struct("EntityMessageSubscriber::Entity")
                .field("handle", handle)
                .finish(),
            EntityMessageSubscriber::Pending(vec) => f
                .debug_struct("EntityMessageSubscriber::Pending")
                .field(
                    "envelopes",
                    &vec.iter()
                        .map(|envelope| envelope.payload_type_name())
                        .collect::<Vec<_>>(),
                )
                .finish(),
        }
    }
}

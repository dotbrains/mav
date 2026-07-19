use anyhow::{Context, Result};
use collections::{HashMap, TypeIdHashMap};
use futures::{
    Future, FutureExt as _, Stream, StreamExt as _,
    channel::oneshot,
    future::{BoxFuture, LocalBoxFuture},
    stream::BoxStream,
};
use gpui::{AnyEntity, AnyWeakEntity, AsyncApp, BackgroundExecutor, Entity, FutureExt as _};
use parking_lot::Mutex;
use proto::{
    AnyTypedEnvelope, EntityMessage, Envelope, EnvelopedMessage, LspRequestId, LspRequestMessage,
    RequestMessage, TypedEnvelope, error::ErrorExt as _,
};
use std::{
    any::{Any, TypeId},
    sync::{
        Arc, OnceLock,
        atomic::{self, AtomicU64},
    },
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct AnyProtoClient(Arc<State>);

type RequestIds = Arc<
    Mutex<
        HashMap<
            LspRequestId,
            oneshot::Sender<
                Result<
                    Option<TypedEnvelope<Vec<proto::ProtoLspResponse<Box<dyn AnyTypedEnvelope>>>>>,
                >,
            >,
        >,
    >,
>;

static NEXT_LSP_REQUEST_ID: OnceLock<Arc<AtomicU64>> = OnceLock::new();
static REQUEST_IDS: OnceLock<RequestIds> = OnceLock::new();

struct State {
    client: Arc<dyn ProtoClient>,
    next_lsp_request_id: Arc<AtomicU64>,
    request_ids: RequestIds,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("next_lsp_request_id", &self.next_lsp_request_id)
            .field("request_ids", &self.request_ids)
            .finish_non_exhaustive()
    }
}

pub trait ProtoClient: Send + Sync {
    fn request(
        &self,
        envelope: Envelope,
        request_type: &'static str,
    ) -> BoxFuture<'static, Result<Envelope>>;

    fn request_stream(
        &self,
        envelope: Envelope,
        request_type: &'static str,
    ) -> BoxFuture<'static, Result<BoxStream<'static, Result<Envelope>>>> {
        async move {
            anyhow::bail!(
                "stream requests are not supported for {request_type}: {:?}",
                envelope.payload
            )
        }
        .boxed()
    }

    fn send(&self, envelope: Envelope, message_type: &'static str) -> Result<()>;

    fn send_response(&self, envelope: Envelope, message_type: &'static str) -> Result<()>;

    fn message_handler_set(&self) -> &parking_lot::Mutex<ProtoMessageHandlerSet>;

    fn is_via_collab(&self) -> bool;
    fn has_wsl_interop(&self) -> bool;
}

mod handler_set;
mod lsp;
pub use handler_set::{EntityMessageSubscriber, ProtoMessageHandlerSet};

impl<T> From<Arc<T>> for AnyProtoClient
where
    T: ProtoClient + 'static,
{
    fn from(client: Arc<T>) -> Self {
        Self::new(client)
    }
}

impl AnyProtoClient {
    pub fn new<T: ProtoClient + 'static>(client: Arc<T>) -> Self {
        Self(Arc::new(State {
            client,
            next_lsp_request_id: NEXT_LSP_REQUEST_ID
                .get_or_init(|| Arc::new(AtomicU64::new(0)))
                .clone(),
            request_ids: REQUEST_IDS.get_or_init(RequestIds::default).clone(),
        }))
    }

    pub fn is_via_collab(&self) -> bool {
        self.0.client.is_via_collab()
    }

    pub fn request<T: RequestMessage>(
        &self,
        request: T,
    ) -> impl Future<Output = Result<T::Response>> + use<T> {
        let envelope = request.into_envelope(0, None, None);
        let response = self.0.client.request(envelope, T::NAME);
        async move {
            T::Response::from_envelope(response.await?)
                .context("received response of the wrong type")
        }
    }

    pub fn request_stream<T: RequestMessage>(
        &self,
        request: T,
    ) -> impl Future<Output = Result<BoxStream<'static, Result<T::Response>>>> + use<T> {
        let envelope = request.into_envelope(0, None, None);
        let response_stream = self.0.client.request_stream(envelope, T::NAME);
        async move {
            Ok(response_stream
                .await?
                .map(|response| {
                    T::Response::from_envelope(response?)
                        .context("received response of the wrong type")
                })
                .boxed())
        }
    }

    pub fn send<T: EnvelopedMessage>(&self, request: T) -> Result<()> {
        let envelope = request.into_envelope(0, None, None);
        self.0.client.send(envelope, T::NAME)
    }

    pub fn send_response<T: EnvelopedMessage>(&self, request_id: u32, request: T) -> Result<()> {
        let envelope = request.into_envelope(0, Some(request_id), None);
        self.0.client.send(envelope, T::NAME)
    }

    pub fn add_request_handler<M, E, H, F>(&self, entity: gpui::WeakEntity<E>, handler: H)
    where
        M: RequestMessage,
        E: 'static,
        H: 'static + Sync + Fn(Entity<E>, TypedEnvelope<M>, AsyncApp) -> F + Send + Sync,
        F: 'static + Future<Output = Result<M::Response>>,
    {
        self.0
            .client
            .message_handler_set()
            .lock()
            .add_message_handler(
                TypeId::of::<M>(),
                entity.into(),
                Arc::new(move |entity, envelope, client, cx| {
                    let entity = entity.downcast::<E>().unwrap();
                    let envelope = envelope.into_any().downcast::<TypedEnvelope<M>>().unwrap();
                    let request_id = envelope.message_id();
                    handler(entity, *envelope, cx)
                        .then(move |result| async move {
                            match result {
                                Ok(response) => {
                                    client.send_response(request_id, response)?;
                                    Ok(())
                                }
                                Err(error) => {
                                    client.send_response(request_id, error.to_proto())?;
                                    Err(error)
                                }
                            }
                        })
                        .boxed_local()
                }),
            )
    }

    pub fn add_entity_request_handler<M, E, H, F>(&self, handler: H)
    where
        M: EnvelopedMessage + RequestMessage + EntityMessage,
        E: 'static,
        H: 'static + Sync + Send + Fn(gpui::Entity<E>, TypedEnvelope<M>, AsyncApp) -> F,
        F: 'static + Future<Output = Result<M::Response>>,
    {
        let message_type_id = TypeId::of::<M>();
        let entity_type_id = TypeId::of::<E>();
        let entity_id_extractor = |envelope: &dyn AnyTypedEnvelope| {
            (envelope as &dyn Any)
                .downcast_ref::<TypedEnvelope<M>>()
                .unwrap()
                .payload
                .remote_entity_id()
        };
        self.0
            .client
            .message_handler_set()
            .lock()
            .add_entity_message_handler(
                message_type_id,
                entity_type_id,
                entity_id_extractor,
                Arc::new(move |entity, envelope, client, cx| {
                    let entity = entity.downcast::<E>().unwrap();
                    let envelope = envelope.into_any().downcast::<TypedEnvelope<M>>().unwrap();
                    let request_id = envelope.message_id();
                    handler(entity, *envelope, cx)
                        .then(move |result| async move {
                            match result {
                                Ok(response) => {
                                    client.send_response(request_id, response)?;
                                    Ok(())
                                }
                                Err(error) => {
                                    client.send_response(request_id, error.to_proto())?;
                                    Err(error)
                                }
                            }
                        })
                        .boxed_local()
                }),
            );
    }

    pub fn add_entity_stream_request_handler<M, E, H, F, S>(&self, handler: H)
    where
        M: EnvelopedMessage + RequestMessage + EntityMessage,
        E: 'static,
        H: 'static + Sync + Send + Fn(gpui::Entity<E>, TypedEnvelope<M>, AsyncApp) -> F,
        F: 'static + Future<Output = Result<S>>,
        S: 'static + Stream<Item = Result<M::Response>>,
    {
        let message_type_id = TypeId::of::<M>();
        let entity_type_id = TypeId::of::<E>();
        let entity_id_extractor = |envelope: &dyn AnyTypedEnvelope| {
            (envelope as &dyn Any)
                .downcast_ref::<TypedEnvelope<M>>()
                .unwrap()
                .payload
                .remote_entity_id()
        };
        self.0
            .client
            .message_handler_set()
            .lock()
            .add_entity_message_handler(
                message_type_id,
                entity_type_id,
                entity_id_extractor,
                Arc::new(move |entity, envelope, client, cx| {
                    let entity = entity.downcast::<E>().unwrap();
                    let envelope = envelope.into_any().downcast::<TypedEnvelope<M>>().unwrap();
                    let request_id = envelope.message_id();
                    let stream = handler(entity, *envelope, cx);
                    async move {
                        // An Error response is itself a terminal stream frame on
                        // both transports (Peer and ChannelClient), so we don't
                        // need to follow it with an EndStream.
                        match stream.await {
                            Ok(stream) => {
                                futures::pin_mut!(stream);
                                while let Some(result) = stream.next().await {
                                    match result {
                                        Ok(response) => {
                                            client.send_response(request_id, response)?
                                        }
                                        Err(error) => {
                                            client.send_response(request_id, error.to_proto())?;
                                            return Err(error);
                                        }
                                    }
                                }
                                client.send_response(request_id, proto::EndStream {})?;
                                Ok(())
                            }
                            Err(error) => {
                                client.send_response(request_id, error.to_proto())?;
                                Err(error)
                            }
                        }
                    }
                    .boxed_local()
                }),
            );
    }

    pub fn add_entity_message_handler<M, E, H, F>(&self, handler: H)
    where
        M: EnvelopedMessage + EntityMessage,
        E: 'static,
        H: 'static + Sync + Send + Fn(gpui::Entity<E>, TypedEnvelope<M>, AsyncApp) -> F,
        F: 'static + Future<Output = Result<()>>,
    {
        let message_type_id = TypeId::of::<M>();
        let entity_type_id = TypeId::of::<E>();
        let entity_id_extractor = |envelope: &dyn AnyTypedEnvelope| {
            (envelope as &dyn Any)
                .downcast_ref::<TypedEnvelope<M>>()
                .unwrap()
                .payload
                .remote_entity_id()
        };
        self.0
            .client
            .message_handler_set()
            .lock()
            .add_entity_message_handler(
                message_type_id,
                entity_type_id,
                entity_id_extractor,
                Arc::new(move |entity, envelope, _, cx| {
                    let entity = entity.downcast::<E>().unwrap();
                    let envelope = envelope.into_any().downcast::<TypedEnvelope<M>>().unwrap();
                    handler(entity, *envelope, cx).boxed_local()
                }),
            );
    }

    pub fn subscribe_to_entity<E: 'static>(&self, remote_id: u64, entity: &Entity<E>) {
        let id = (TypeId::of::<E>(), remote_id);

        let mut message_handlers = self.0.client.message_handler_set().lock();
        if message_handlers
            .entities_by_type_and_remote_id
            .contains_key(&id)
        {
            panic!("already subscribed to entity");
        }

        message_handlers.entities_by_type_and_remote_id.insert(
            id,
            EntityMessageSubscriber::Entity {
                handle: entity.downgrade().into(),
            },
        );
    }

    pub fn has_wsl_interop(&self) -> bool {
        self.0.client.has_wsl_interop()
    }
}

#[cfg(any(test, feature = "test-support"))]
pub struct NoopProtoClient {
    handler_set: parking_lot::Mutex<ProtoMessageHandlerSet>,
}

#[cfg(any(test, feature = "test-support"))]
impl NoopProtoClient {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            handler_set: parking_lot::Mutex::new(ProtoMessageHandlerSet::default()),
        })
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ProtoClient for NoopProtoClient {
    fn request(
        &self,
        _: proto::Envelope,
        _: &'static str,
    ) -> futures::future::BoxFuture<'static, Result<proto::Envelope>> {
        unimplemented!()
    }
    fn send(&self, _: proto::Envelope, _: &'static str) -> Result<()> {
        Ok(())
    }
    fn send_response(&self, _: proto::Envelope, _: &'static str) -> Result<()> {
        Ok(())
    }
    fn message_handler_set(&self) -> &parking_lot::Mutex<ProtoMessageHandlerSet> {
        &self.handler_set
    }
    fn is_via_collab(&self) -> bool {
        false
    }
    fn has_wsl_interop(&self) -> bool {
        false
    }
}

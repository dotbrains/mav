use super::*;
use crate::remote_client::connection::{ResponseChannels, StreamResponseChannels};

struct Signal<T: 'static> {
    tx: Mutex<Option<oneshot::Sender<T>>>,
    rx: Shared<Task<Option<T>>>,
}

impl<T: Send + Clone + 'static> Signal<T> {
    pub fn new(cx: &App) -> Self {
        let (tx, rx) = oneshot::channel();

        let task = cx
            .background_executor()
            .spawn(async move { rx.await.ok() })
            .shared();

        Self {
            tx: Mutex::new(Some(tx)),
            rx: task,
        }
    }

    fn set(&self, value: T) {
        if let Some(tx) = self.tx.lock().take() {
            let _ = tx.send(value);
        }
    }

    fn wait(&self) -> Shared<Task<Option<T>>> {
        self.rx.clone()
    }
}

pub(crate) struct ChannelClient {
    next_message_id: AtomicU32,
    outgoing_tx: Mutex<mpsc::UnboundedSender<Envelope>>,
    buffer: Mutex<VecDeque<Envelope>>,
    response_channels: ResponseChannels,
    stream_response_channels: StreamResponseChannels,
    message_handlers: Mutex<ProtoMessageHandlerSet>,
    max_received: AtomicU32,
    name: &'static str,
    task: Mutex<Task<Result<()>>>,
    remote_started: Signal<()>,
    has_wsl_interop: bool,
    executor: BackgroundExecutor,
}

impl ChannelClient {
    pub(crate) fn new(
        incoming_rx: mpsc::UnboundedReceiver<Envelope>,
        outgoing_tx: mpsc::UnboundedSender<Envelope>,
        cx: &App,
        name: &'static str,
        has_wsl_interop: bool,
    ) -> Arc<Self> {
        Arc::new_cyclic(|this| Self {
            outgoing_tx: Mutex::new(outgoing_tx),
            next_message_id: AtomicU32::new(0),
            max_received: AtomicU32::new(0),
            response_channels: ResponseChannels::default(),
            stream_response_channels: StreamResponseChannels::default(),
            message_handlers: Default::default(),
            buffer: Mutex::new(VecDeque::new()),
            name,
            executor: cx.background_executor().clone(),
            task: Mutex::new(Self::start_handling_messages(
                this.clone(),
                incoming_rx,
                &cx.to_async(),
            )),
            remote_started: Signal::new(cx),
            has_wsl_interop,
        })
    }

    pub(super) fn wait_for_remote_started(&self) -> Shared<Task<Option<()>>> {
        self.remote_started.wait()
    }

    fn start_handling_messages(
        this: Weak<Self>,
        mut incoming_rx: mpsc::UnboundedReceiver<Envelope>,
        cx: &AsyncApp,
    ) -> Task<Result<()>> {
        cx.spawn(async move |cx| {
            if let Some(this) = this.upgrade() {
                let envelope = proto::RemoteStarted {}.into_envelope(0, None, None);
                this.outgoing_tx.lock().unbounded_send(envelope).ok();
            };

            let peer_id = PeerId { owner_id: 0, id: 0 };
            while let Some(incoming) = incoming_rx.next().await {
                let Some(this) = this.upgrade() else {
                    return anyhow::Ok(());
                };
                if let Some(ack_id) = incoming.ack_id {
                    let mut buffer = this.buffer.lock();
                    while buffer.front().is_some_and(|msg| msg.id <= ack_id) {
                        buffer.pop_front();
                    }
                }
                if let Some(proto::envelope::Payload::FlushBufferedMessages(_)) = &incoming.payload
                {
                    log::debug!(
                        "{}:remote message received. name:FlushBufferedMessages",
                        this.name
                    );
                    {
                        let buffer = this.buffer.lock();
                        for envelope in buffer.iter() {
                            this.outgoing_tx
                                .lock()
                                .unbounded_send(envelope.clone())
                                .ok();
                        }
                    }
                    let mut envelope = proto::Ack {}.into_envelope(0, Some(incoming.id), None);
                    envelope.id = this.next_message_id.fetch_add(1, SeqCst);
                    this.outgoing_tx.lock().unbounded_send(envelope).ok();
                    continue;
                }

                if let Some(proto::envelope::Payload::RemoteStarted(_)) = &incoming.payload {
                    this.remote_started.set(());
                    let mut envelope = proto::Ack {}.into_envelope(0, Some(incoming.id), None);
                    envelope.id = this.next_message_id.fetch_add(1, SeqCst);
                    this.outgoing_tx.lock().unbounded_send(envelope).ok();
                    continue;
                }

                this.max_received.store(incoming.id, SeqCst);

                if let Some(request_id) = incoming.responding_to {
                    let request_id = MessageId(request_id);
                    // An incoming response with no payload is malformed; drop
                    // it. The request future and any stream consumers will
                    // remain pending until either a real response arrives or
                    // the connection is torn down.
                    if incoming.payload.is_none() {
                        continue;
                    }
                    let sender = this.response_channels.lock().remove(&request_id);
                    if let Some(sender) = sender {
                        let (tx, rx) = oneshot::channel();
                        sender.send((incoming, tx)).ok();
                        rx.await.ok();
                    } else {
                        let terminal_stream_response = matches!(
                            &incoming.payload,
                            Some(proto::envelope::Payload::Error(_))
                                | Some(proto::envelope::Payload::EndStream(_))
                        );
                        let sender = if terminal_stream_response {
                            this.stream_response_channels.lock().remove(&request_id)
                        } else {
                            this.stream_response_channels
                                .lock()
                                .get(&request_id)
                                .cloned()
                        };
                        if let Some(sender) = sender {
                            let (tx, rx) = oneshot::channel();
                            if sender.unbounded_send((Ok(incoming), tx)).is_err() {
                                this.stream_response_channels.lock().remove(&request_id);
                                continue;
                            }
                            rx.await.ok();
                        }
                    }
                } else if let Some(envelope) =
                    build_typed_envelope(peer_id, Instant::now(), incoming)
                {
                    let type_name = envelope.payload_type_name();
                    let message_id = envelope.message_id();
                    if let Some(future) = ProtoMessageHandlerSet::handle_message(
                        &this.message_handlers,
                        envelope,
                        this.clone().into(),
                        cx.clone(),
                    ) {
                        log::debug!("{}:remote message received. name:{type_name}", this.name);
                        cx.foreground_executor()
                            .spawn(async move {
                                match future.await {
                                    Ok(_) => {
                                        log::debug!(
                                            "{}:remote message handled. name:{type_name}",
                                            this.name
                                        );
                                    }
                                    Err(error) => {
                                        log::error!(
                                            "{}:error handling message. type:{}, error:{:#}",
                                            this.name,
                                            type_name,
                                            format!("{error:#}").lines().fold(
                                                String::new(),
                                                |mut message, line| {
                                                    if !message.is_empty() {
                                                        message.push(' ');
                                                    }
                                                    message.push_str(line);
                                                    message
                                                }
                                            )
                                        );
                                    }
                                }
                            })
                            .detach()
                    } else {
                        log::error!("{}:unhandled remote message name:{type_name}", this.name);
                        if let Err(e) = AnyProtoClient::from(this.clone()).send_response(
                            message_id,
                            anyhow::anyhow!("no handler registered for {type_name}").to_proto(),
                        ) {
                            log::error!(
                                "{}:error sending error response for {type_name}:{e:#}",
                                this.name
                            );
                        }
                    }
                }
            }
            anyhow::Ok(())
        })
    }

    pub(crate) fn reconnect(
        self: &Arc<Self>,
        incoming_rx: UnboundedReceiver<Envelope>,
        outgoing_tx: UnboundedSender<Envelope>,
        cx: &AsyncApp,
    ) {
        *self.outgoing_tx.lock() = outgoing_tx;
        *self.task.lock() = Self::start_handling_messages(Arc::downgrade(self), incoming_rx, cx);
    }

    fn request<T: RequestMessage>(
        &self,
        payload: T,
    ) -> impl 'static + Future<Output = Result<T::Response>> {
        self.request_internal(payload, true)
    }

    fn request_internal<T: RequestMessage>(
        &self,
        payload: T,
        use_buffer: bool,
    ) -> impl 'static + Future<Output = Result<T::Response>> {
        log::debug!("remote request start. name:{}", T::NAME);
        let response =
            self.request_dynamic(payload.into_envelope(0, None, None), T::NAME, use_buffer);
        async move {
            let response = response.await?;
            log::debug!("remote request finish. name:{}", T::NAME);
            T::Response::from_envelope(response).context("received a response of the wrong type")
        }
    }

    pub(super) async fn resync(&self, timeout: Duration) -> Result<()> {
        smol::future::or(
            async {
                self.request_internal(proto::FlushBufferedMessages {}, false)
                    .await?;

                for envelope in self.buffer.lock().iter() {
                    self.outgoing_tx
                        .lock()
                        .unbounded_send(envelope.clone())
                        .ok();
                }
                Ok(())
            },
            async {
                self.executor.timer(timeout).await;
                anyhow::bail!("Timed out resyncing remote client")
            },
        )
        .await
    }

    pub(super) async fn ping(&self, timeout: Duration) -> Result<()> {
        smol::future::or(
            async {
                self.request(proto::Ping {}).await?;
                Ok(())
            },
            async {
                self.executor.timer(timeout).await;
                anyhow::bail!("Timed out pinging remote client")
            },
        )
        .await
    }

    pub(super) fn send<T: EnvelopedMessage>(&self, payload: T) -> Result<()> {
        log::debug!("remote send name:{}", T::NAME);
        self.send_dynamic(payload.into_envelope(0, None, None))
    }

    fn request_dynamic(
        &self,
        mut envelope: proto::Envelope,
        type_name: &'static str,
        use_buffer: bool,
    ) -> impl 'static + Future<Output = Result<proto::Envelope>> {
        envelope.id = self.next_message_id.fetch_add(1, SeqCst);
        let (tx, rx) = oneshot::channel();
        let mut response_channels_lock = self.response_channels.lock();
        response_channels_lock.insert(MessageId(envelope.id), tx);
        drop(response_channels_lock);

        let result = if use_buffer {
            self.send_buffered(envelope)
        } else {
            self.send_unbuffered(envelope)
        };
        async move {
            if let Err(error) = &result {
                log::error!("failed to send message: {error}");
                anyhow::bail!("failed to send message: {error}");
            }

            let response = rx.await.context("connection lost")?.0;
            if let Some(proto::envelope::Payload::Error(error)) = &response.payload {
                return Err(RpcError::from_proto(&error, type_name));
            }
            Ok(response)
        }
    }

    fn request_stream_dynamic(
        &self,
        mut envelope: proto::Envelope,
        type_name: &'static str,
    ) -> impl 'static + Future<Output = Result<BoxStream<'static, Result<proto::Envelope>>>> {
        envelope.id = self.next_message_id.fetch_add(1, SeqCst);
        let message_id = MessageId(envelope.id);
        let (tx, rx) = mpsc::unbounded();
        let stream_response_channels = self.stream_response_channels.clone();
        stream_response_channels.lock().insert(message_id, tx);

        let result = self.send_buffered(envelope);
        async move {
            if let Err(error) = &result {
                log::error!("failed to send message: {error}");
                anyhow::bail!("failed to send message: {error}");
            }

            let cleanup_stream_response_channel = util::defer({
                let stream_response_channels = stream_response_channels.clone();
                move || {
                    stream_response_channels.lock().remove(&message_id);
                }
            });

            Ok(rx
                .filter_map(move |(response, _barrier)| {
                    // Keep the cleanup guard alive until the returned stream is dropped.
                    let _keep_cleanup_guard_alive = &cleanup_stream_response_channel;
                    futures::future::ready(match response {
                        Ok(response) => {
                            if let Some(proto::envelope::Payload::Error(error)) = &response.payload
                            {
                                Some(Err(RpcError::from_proto(&error, type_name)))
                            } else if let Some(proto::envelope::Payload::EndStream(_)) =
                                &response.payload
                            {
                                None
                            } else {
                                Some(Ok(response))
                            }
                        }
                        Err(error) => Some(Err(error)),
                    })
                })
                .boxed())
        }
    }

    pub fn send_dynamic(&self, mut envelope: proto::Envelope) -> Result<()> {
        envelope.id = self.next_message_id.fetch_add(1, SeqCst);
        self.send_buffered(envelope)
    }

    fn send_buffered(&self, mut envelope: proto::Envelope) -> Result<()> {
        envelope.ack_id = Some(self.max_received.load(SeqCst));
        self.buffer.lock().push_back(envelope.clone());
        // ignore errors on send (happen while we're reconnecting)
        // assume that the global "disconnected" overlay is sufficient.
        self.outgoing_tx.lock().unbounded_send(envelope).ok();
        Ok(())
    }

    fn send_unbuffered(&self, mut envelope: proto::Envelope) -> Result<()> {
        envelope.ack_id = Some(self.max_received.load(SeqCst));
        self.outgoing_tx.lock().unbounded_send(envelope).ok();
        Ok(())
    }
}

impl ProtoClient for ChannelClient {
    fn request(
        &self,
        envelope: proto::Envelope,
        request_type: &'static str,
    ) -> BoxFuture<'static, Result<proto::Envelope>> {
        self.request_dynamic(envelope, request_type, true).boxed()
    }

    fn request_stream(
        &self,
        envelope: proto::Envelope,
        request_type: &'static str,
    ) -> BoxFuture<'static, Result<BoxStream<'static, Result<proto::Envelope>>>> {
        self.request_stream_dynamic(envelope, request_type).boxed()
    }

    fn send(&self, envelope: proto::Envelope, _message_type: &'static str) -> Result<()> {
        self.send_dynamic(envelope)
    }

    fn send_response(&self, envelope: Envelope, _message_type: &'static str) -> anyhow::Result<()> {
        self.send_dynamic(envelope)
    }

    fn message_handler_set(&self) -> &Mutex<ProtoMessageHandlerSet> {
        &self.message_handlers
    }

    fn is_via_collab(&self) -> bool {
        false
    }

    fn has_wsl_interop(&self) -> bool {
        self.has_wsl_interop
    }
}

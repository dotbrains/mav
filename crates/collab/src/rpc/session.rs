use super::*;
use crate::entities::User;
use core::fmt::{self, Debug, Formatter};
use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    rc::Rc,
};

#[derive(Clone, Debug)]
pub enum Principal {
    User(User),
}

impl Principal {
    pub(super) fn update_span(&self, span: &tracing::Span) {
        match &self {
            Principal::User(user) => {
                span.record("user_id", user.id.0);
                span.record("username", &user.username);
                span.record("login", &user.github_login);
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct MessageContext {
    pub(super) session: Session,
    pub(super) span: tracing::Span,
}

impl Deref for MessageContext {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl MessageContext {
    pub fn forward_request<T: RequestMessage>(
        &self,
        receiver_id: ConnectionId,
        request: T,
    ) -> impl Future<Output = anyhow::Result<T::Response>> {
        let request_start_time = Instant::now();
        let span = self.span.clone();
        tracing::info!("start forwarding request");
        self.peer
            .forward_request(self.connection_id, receiver_id, request)
            .inspect(move |_| {
                span.record(
                    HOST_WAITING_MS,
                    request_start_time.elapsed().as_micros() as f64 / 1000.0,
                );
            })
            .inspect_err(|_| tracing::error!("error forwarding request"))
            .inspect_ok(|_| tracing::info!("finished forwarding request"))
    }

    pub fn forward_request_stream<T: RequestMessage>(
        &self,
        receiver_id: ConnectionId,
        request: T,
    ) -> impl Future<Output = anyhow::Result<BoxStream<'static, anyhow::Result<T::Response>>>> {
        let request_start_time = Instant::now();
        let span = self.span.clone();
        let peer = self.peer.clone();
        let envelope = request.into_envelope(0, None, Some(self.connection_id.into()));
        async move {
            tracing::info!("start forwarding stream request");
            let stream = peer
                .request_stream_dynamic(receiver_id, envelope, T::NAME)
                .await;
            span.record(
                HOST_WAITING_MS,
                request_start_time.elapsed().as_micros() as f64 / 1000.0,
            );
            let stream = stream
                .inspect_err(|_| tracing::error!("error forwarding stream request"))?
                .map(|response| {
                    T::Response::from_envelope(response?)
                        .context("received response of the wrong type")
                })
                .boxed();
            tracing::info!("finished opening forwarded stream request");
            Ok(stream)
        }
    }
}

#[derive(Clone)]
pub(super) struct Session {
    pub(super) principal: Principal,
    pub(super) connection_id: ConnectionId,
    pub(super) db: Arc<tokio::sync::Mutex<DbHandle>>,
    pub(super) peer: Arc<Peer>,
    pub(super) connection_pool: Arc<parking_lot::Mutex<ConnectionPool>>,
    pub(super) app_state: Arc<AppState>,
    /// The GeoIP country code for the user.
    #[allow(unused)]
    pub(super) geoip_country_code: Option<String>,
    #[allow(unused)]
    pub(super) system_id: Option<String>,
    pub(super) _executor: Executor,
}

impl Session {
    pub(super) async fn db(&self) -> tokio::sync::MutexGuard<'_, DbHandle> {
        #[cfg(feature = "test-support")]
        tokio::task::yield_now().await;
        let guard = self.db.lock().await;
        #[cfg(feature = "test-support")]
        tokio::task::yield_now().await;
        guard
    }

    pub(super) async fn connection_pool(&self) -> ConnectionPoolGuard<'_> {
        #[cfg(feature = "test-support")]
        tokio::task::yield_now().await;
        let guard = self.connection_pool.lock();
        ConnectionPoolGuard {
            guard,
            _not_send: PhantomData,
        }
    }

    #[expect(dead_code)]
    pub(super) fn is_staff(&self) -> bool {
        match &self.principal {
            Principal::User(user) => user.admin,
        }
    }

    pub(super) fn user_id(&self) -> UserId {
        match &self.principal {
            Principal::User(user) => user.id,
        }
    }
}

impl Debug for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut result = f.debug_struct("Session");
        match &self.principal {
            Principal::User(user) => {
                result.field("user", &user.username);
            }
        }
        result.field("connection_id", &self.connection_id).finish()
    }
}

pub(super) struct DbHandle(pub(super) Arc<Database>);

impl Deref for DbHandle {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

pub(super) struct ConnectionPoolGuard<'a> {
    guard: parking_lot::MutexGuard<'a, ConnectionPool>,
    _not_send: PhantomData<Rc<()>>,
}

impl Deref for ConnectionPoolGuard<'_> {
    type Target = ConnectionPool;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl DerefMut for ConnectionPoolGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl Drop for ConnectionPoolGuard<'_> {
    fn drop(&mut self) {
        #[cfg(feature = "test-support")]
        self.check_invariants();
    }
}

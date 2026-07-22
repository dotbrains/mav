use crate::Result;

use rpc::{Peer, Receipt, proto::RequestMessage};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
};

const MAX_CONCURRENT_CONNECTIONS: usize = 512;
static CONCURRENT_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

pub struct ConnectionGuard;

impl ConnectionGuard {
    pub fn try_acquire() -> Result<Self, ()> {
        let current_connections = CONCURRENT_CONNECTIONS.fetch_add(1, SeqCst);
        if current_connections >= MAX_CONCURRENT_CONNECTIONS {
            CONCURRENT_CONNECTIONS.fetch_sub(1, SeqCst);
            tracing::error!(
                "too many concurrent connections: {}",
                current_connections + 1
            );
            return Err(());
        }
        Ok(ConnectionGuard)
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        CONCURRENT_CONNECTIONS.fetch_sub(1, SeqCst);
    }
}

pub(super) struct Response<R> {
    pub(super) peer: Arc<Peer>,
    pub(super) receipt: Receipt<R>,
    pub(super) responded: Arc<AtomicBool>,
}

impl<R: RequestMessage> Response<R> {
    pub(super) fn send(self, payload: R::Response) -> Result<()> {
        self.responded.store(true, SeqCst);
        self.peer.respond(self.receipt, payload)?;
        Ok(())
    }
}

pub(super) struct StreamResponse<R> {
    pub(super) peer: Arc<Peer>,
    pub(super) receipt: Receipt<R>,
    pub(super) ended: Arc<AtomicBool>,
}

impl<R: RequestMessage> StreamResponse<R> {
    pub(super) fn send(&self, payload: R::Response) -> Result<()> {
        self.peer.respond(self.receipt, payload)?;
        Ok(())
    }

    pub(super) fn end(self) -> Result<()> {
        // Always mark `ended` even if sending `EndStream` on the wire fails, so that
        // `ended` reflects "the handler intended to end the stream". The caller still
        // gets the underlying error and routes through the Err arm of the handler,
        // which sends `respond_with_error` to terminate the client-side stream.
        let result = self.peer.end_stream(self.receipt);
        self.ended.store(true, SeqCst);
        result?;
        Ok(())
    }
}

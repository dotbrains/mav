use super::{AcpSession, AcpSessionList};
use agent_client_protocol::schema::v1 as acp;
use agent_client_protocol::{Agent, ConnectionTo, JsonRpcResponse, Responder};
use collections::HashMap;
use futures::channel::mpsc;
use gpui::AsyncApp;
use std::cell::RefCell;
use std::rc::Rc;
use util::ResultExt as _;

/// Holds state needed by foreground work dispatched from background handler closures.
pub(super) struct ClientContext {
    pub(super) sessions: Rc<RefCell<HashMap<acp::SessionId, AcpSession>>>,
    pub(super) session_list: Rc<RefCell<Option<Rc<AcpSessionList>>>>,
}

fn dispatch_queue_closed_error() -> acp::Error {
    acp::Error::internal_error().data("ACP foreground dispatch queue closed")
}

/// Work items sent from `Send` handler closures to the `!Send` foreground thread.
pub(super) trait ForegroundWorkItem: Send {
    fn run(self: Box<Self>, cx: &mut AsyncApp, ctx: &ClientContext);
    fn reject(self: Box<Self>);
}

pub(super) type ForegroundWork = Box<dyn ForegroundWorkItem>;

struct RequestForegroundWork<Req, Res>
where
    Req: Send + 'static,
    Res: JsonRpcResponse + Send + 'static,
{
    request: Req,
    responder: Responder<Res>,
    handler: fn(Req, Responder<Res>, &mut AsyncApp, &ClientContext),
}

impl<Req, Res> ForegroundWorkItem for RequestForegroundWork<Req, Res>
where
    Req: Send + 'static,
    Res: JsonRpcResponse + Send + 'static,
{
    fn run(self: Box<Self>, cx: &mut AsyncApp, ctx: &ClientContext) {
        let Self {
            request,
            responder,
            handler,
        } = *self;
        handler(request, responder, cx, ctx);
    }

    fn reject(self: Box<Self>) {
        let Self { responder, .. } = *self;
        log::error!("ACP foreground dispatch queue closed while handling inbound request");
        responder
            .respond_with_error(dispatch_queue_closed_error())
            .log_err();
    }
}

struct NotificationForegroundWork<Notif>
where
    Notif: Send + 'static,
{
    notification: Notif,
    connection: ConnectionTo<Agent>,
    handler: fn(Notif, &mut AsyncApp, &ClientContext),
}

impl<Notif> ForegroundWorkItem for NotificationForegroundWork<Notif>
where
    Notif: Send + 'static,
{
    fn run(self: Box<Self>, cx: &mut AsyncApp, ctx: &ClientContext) {
        let Self {
            notification,
            handler,
            ..
        } = *self;
        handler(notification, cx, ctx);
    }

    fn reject(self: Box<Self>) {
        let Self { connection, .. } = *self;
        log::error!("ACP foreground dispatch queue closed while handling inbound notification");
        connection
            .send_error_notification(dispatch_queue_closed_error())
            .log_err();
    }
}

pub(super) fn enqueue_request<Req, Res>(
    dispatch_tx: &mpsc::UnboundedSender<ForegroundWork>,
    request: Req,
    responder: Responder<Res>,
    handler: fn(Req, Responder<Res>, &mut AsyncApp, &ClientContext),
) where
    Req: Send + 'static,
    Res: JsonRpcResponse + Send + 'static,
{
    let work: ForegroundWork = Box::new(RequestForegroundWork {
        request,
        responder,
        handler,
    });
    if let Err(err) = dispatch_tx.unbounded_send(work) {
        err.into_inner().reject();
    }
}

pub(super) fn enqueue_notification<Notif>(
    dispatch_tx: &mpsc::UnboundedSender<ForegroundWork>,
    notification: Notif,
    connection: ConnectionTo<Agent>,
    handler: fn(Notif, &mut AsyncApp, &ClientContext),
) where
    Notif: Send + 'static,
{
    let work: ForegroundWork = Box::new(NotificationForegroundWork {
        notification,
        connection,
        handler,
    });
    if let Err(err) = dispatch_tx.unbounded_send(work) {
        err.into_inner().reject();
    }
}

//! End-to-end tests for the proxy crate.
//!
//! Each test spawns a real proxy on `127.0.0.1:0` and makes real TCP
//! connections to it, optionally also spawning a tiny stub origin server
//! (or stub upstream proxy) to act as the destination. Everything is sync —
//! std::net + threads + std::time::Duration timeouts.

use futures::channel::mpsc;
use futures::stream::StreamExt;
pub use http_proxy::{
    Allowlist, DenyReason, HostPattern, ProxyConfig, ProxyEvent, ProxyHandle, RequestMethod,
    RequestOutcome, UpstreamProxy,
};
pub use std::io::{Read, Write};
pub use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
#[cfg(unix)]
pub use std::os::unix::net::UnixStream;
use std::thread;
pub use std::time::Duration;

pub const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Spin up a tiny TCP server that serves one connection: it reads the
/// client's first request (until `\r\n\r\n`), echoes a fixed HTTP
/// response, and returns the request bytes it saw.
pub fn spawn_echo_origin(response: &'static [u8]) -> (SocketAddr, thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let join = thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
        let buf = read_until_double_crlf(&mut sock);
        sock.write_all(response).unwrap();
        sock.shutdown(std::net::Shutdown::Write).unwrap();
        buf
    });
    (addr, join)
}

/// Spin up a stub upstream HTTP proxy that serves one connection: it reads
/// a CONNECT request, replies `200`, then acts like the requested origin —
/// reading one more request through the "tunnel" and echoing a fixed
/// response. Returns the CONNECT headers and the tunneled request bytes.
pub fn spawn_stub_upstream_proxy(
    tunnel_response: &'static [u8],
) -> (SocketAddr, thread::JoinHandle<(Vec<u8>, Vec<u8>)>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let join = thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
        let connect_request = read_until_double_crlf(&mut sock);
        assert!(
            connect_request.starts_with(b"CONNECT "),
            "expected CONNECT, got: {:?}",
            String::from_utf8_lossy(&connect_request)
        );
        sock.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .unwrap();
        let tunneled_request = read_until_double_crlf(&mut sock);
        sock.write_all(tunnel_response).unwrap();
        sock.shutdown(std::net::Shutdown::Write).unwrap();
        (connect_request, tunneled_request)
    });
    (addr, join)
}

pub fn read_until_double_crlf(sock: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    loop {
        let n = sock.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    buf
}

pub fn spawn_proxy(allowlist: Allowlist) -> (ProxyHandle, mpsc::UnboundedReceiver<ProxyEvent>) {
    spawn_proxy_with_upstream(allowlist, None)
}

pub fn spawn_proxy_with_upstream(
    allowlist: Allowlist,
    upstream: Option<UpstreamProxy>,
) -> (ProxyHandle, mpsc::UnboundedReceiver<ProxyEvent>) {
    let (events_tx, mut events_rx) = mpsc::unbounded();
    let proxy = ProxyHandle::spawn(ProxyConfig {
        allowlist,
        upstream,
        events: events_tx,
    })
    .expect("proxy spawn");

    drain_ready(&proxy, &mut events_rx);
    (proxy, events_rx)
}

#[cfg(unix)]
pub fn spawn_unix_proxy(
    allowlist: Allowlist,
) -> (ProxyHandle, mpsc::UnboundedReceiver<ProxyEvent>) {
    let (events_tx, mut events_rx) = mpsc::unbounded();
    let proxy = ProxyHandle::spawn_unix_temp(ProxyConfig {
        allowlist,
        upstream: None,
        events: events_tx,
    })
    .expect("proxy spawn");

    drain_ready(&proxy, &mut events_rx);
    (proxy, events_rx)
}

pub fn drain_ready(proxy: &ProxyHandle, events_rx: &mut mpsc::UnboundedReceiver<ProxyEvent>) {
    let ready = futures::executor::block_on(events_rx.next());
    match ready {
        Some(ProxyEvent::Ready { port }) => assert_eq!(port, proxy.port()),
        other => panic!("expected Ready event first, got {other:?}"),
    }
}

pub fn next_event(events: &mut mpsc::UnboundedReceiver<ProxyEvent>) -> ProxyEvent {
    futures::executor::block_on(events.next()).expect("events channel closed")
}

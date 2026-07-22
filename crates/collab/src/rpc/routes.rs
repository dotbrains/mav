use super::{
    ConnectionGuard, MavVersion, Principal, Server, headers::AppVersionHeader,
    headers::ProtocolVersion, headers::ReleaseChannelHeader, updates::to_axum_message,
    updates::to_tungstenite_message,
};
use crate::api::{CloudflareIpCountryHeader, SystemIdHeader};
use crate::{Result, auth, executor::Executor};
use anyhow::anyhow;
use axum::headers::UserAgent;
use axum::{
    Extension, Router, TypedHeader,
    body::Body,
    extract::{ConnectInfo, WebSocketUpgrade},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, TryStreamExt};
use prometheus::{IntGauge, register_int_gauge};
use rpc::Connection;
use std::{
    net::SocketAddr,
    sync::{Arc, OnceLock},
};
use tower::ServiceBuilder;

pub fn routes(server: Arc<Server>) -> Router<(), Body> {
    Router::new()
        .route("/rpc", get(handle_websocket_request))
        .layer(
            ServiceBuilder::new()
                .layer(Extension(server.app_state.clone()))
                .layer(middleware::from_fn(auth::validate_header)),
        )
        .route("/metrics", get(handle_metrics))
        .layer(Extension(server))
}

async fn handle_websocket_request(
    TypedHeader(ProtocolVersion(protocol_version)): TypedHeader<ProtocolVersion>,
    app_version_header: Option<TypedHeader<AppVersionHeader>>,
    release_channel_header: Option<TypedHeader<ReleaseChannelHeader>>,
    ConnectInfo(socket_address): ConnectInfo<SocketAddr>,
    Extension(server): Extension<Arc<Server>>,
    Extension(principal): Extension<Principal>,
    user_agent: Option<TypedHeader<UserAgent>>,
    country_code_header: Option<TypedHeader<CloudflareIpCountryHeader>>,
    system_id_header: Option<TypedHeader<SystemIdHeader>>,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    if protocol_version != rpc::PROTOCOL_VERSION {
        return (
            StatusCode::UPGRADE_REQUIRED,
            "client must be upgraded".to_string(),
        )
            .into_response();
    }

    let Some(version) = app_version_header.map(|header| MavVersion(header.0.0)) else {
        return (
            StatusCode::UPGRADE_REQUIRED,
            "no version header found".to_string(),
        )
            .into_response();
    };

    let release_channel = release_channel_header.map(|header| header.0.0);

    if !version.can_collaborate() {
        return (
            StatusCode::UPGRADE_REQUIRED,
            "client must be upgraded".to_string(),
        )
            .into_response();
    }

    let socket_address = socket_address.to_string();

    let connection_guard = match ConnectionGuard::try_acquire() {
        Ok(guard) => guard,
        Err(()) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Too many concurrent connections",
            )
                .into_response();
        }
    };

    ws.on_upgrade(move |socket| {
        let socket = socket
            .map_ok(to_tungstenite_message)
            .err_into()
            .with(|message| async move { to_axum_message(message) });
        let connection = Connection::new(Box::pin(socket));
        async move {
            server
                .handle_connection(
                    connection,
                    socket_address,
                    principal,
                    version,
                    release_channel,
                    user_agent.map(|header| header.to_string()),
                    country_code_header.map(|header| header.to_string()),
                    system_id_header.map(|header| header.to_string()),
                    None,
                    Executor::Production,
                    Some(connection_guard),
                )
                .await;
        }
    })
}

async fn handle_metrics(Extension(server): Extension<Arc<Server>>) -> Result<String> {
    static CONNECTIONS_METRIC: OnceLock<IntGauge> = OnceLock::new();
    let connections_metric = CONNECTIONS_METRIC
        .get_or_init(|| register_int_gauge!("connections", "number of connections").unwrap());

    let connections = server
        .connection_pool
        .lock()
        .connections()
        .filter(|connection| !connection.admin)
        .count();
    connections_metric.set(connections as _);

    static SHARED_PROJECTS_METRIC: OnceLock<IntGauge> = OnceLock::new();
    let shared_projects_metric = SHARED_PROJECTS_METRIC.get_or_init(|| {
        register_int_gauge!(
            "shared_projects",
            "number of open projects with one or more guests"
        )
        .unwrap()
    });

    let shared_projects = server.app_state.db.project_count_excluding_admins().await?;
    shared_projects_metric.set(shared_projects as _);

    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let encoded_metrics = encoder
        .encode_to_string(&metric_families)
        .map_err(|err| anyhow!("{err}"))?;
    Ok(encoded_metrics)
}

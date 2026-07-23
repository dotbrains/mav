use super::*;

pub(super) async fn resolve_start_failure(
    id: &ContextServerId,
    err: anyhow::Error,
    server: Arc<ContextServer>,
    configuration: Arc<ContextServerConfiguration>,
    cx: &AsyncApp,
) -> ContextServerState {
    let www_authenticate = err.downcast_ref::<TransportError>().map(|e| match e {
        TransportError::AuthRequired { www_authenticate } => www_authenticate.clone(),
    });

    if www_authenticate.is_some() && configuration.has_static_auth_header() {
        log::warn!("{id} received 401 with a static Authorization header configured");
        return ContextServerState::Error {
            configuration,
            server,
            error: "Server returned 401 Unauthorized. Check your configured Authorization header."
                .into(),
        };
    }

    let server_url = match configuration.as_ref() {
        ContextServerConfiguration::Http { url, .. } if !configuration.has_static_auth_header() => {
            url.clone()
        }
        _ => {
            if www_authenticate.is_some() {
                log::error!("{id} got OAuth 401 on a non-HTTP transport or with static auth");
            } else {
                log::error!("{id} context server failed to start: {err}");
            }
            return ContextServerState::Error {
                configuration,
                server,
                error: err.to_string().into(),
            };
        }
    };

    // When the error is NOT a 401 but there is a cached OAuth session in the
    // keychain, the session is likely stale/expired and caused the failure
    // (e.g. timeout because the server rejected the token silently). Clear it
    // so the next start attempt can get a clean 401 and trigger the auth flow.
    if www_authenticate.is_none() {
        let credentials_provider = cx.update(|cx| mav_credentials_provider::global(cx));
        match ContextServerStore::load_session(&credentials_provider, &server_url, cx).await {
            Ok(Some(_)) => {
                log::info!("{id} start failed with a cached OAuth session present; clearing it");
                ContextServerStore::clear_session(&credentials_provider, &server_url, cx)
                    .await
                    .log_err();
            }
            _ => {
                log::error!("{id} context server failed to start: {err}");
                return ContextServerState::Error {
                    configuration,
                    server,
                    error: err.to_string().into(),
                };
            }
        }
    }

    let default_www_authenticate = oauth::WwwAuthenticate {
        resource_metadata: None,
        scope: None,
        error: None,
        error_description: None,
    };
    let www_authenticate = www_authenticate
        .as_ref()
        .unwrap_or(&default_www_authenticate);
    let http_client = cx.update(|cx| cx.http_client());

    match context_server::oauth::discover(&http_client, &server_url, www_authenticate).await {
        Ok(discovery) => {
            use context_server::oauth::{
                ClientRegistrationStrategy, determine_registration_strategy,
            };

            let has_preregistered_client_id = matches!(
                configuration.as_ref(),
                ContextServerConfiguration::Http { oauth: Some(_), .. }
            );

            let strategy = determine_registration_strategy(&discovery.auth_server_metadata);

            if matches!(strategy, ClientRegistrationStrategy::Unavailable)
                && !has_preregistered_client_id
            {
                log::error!(
                    "{id} authorization server supports neither CIMD nor DCR, \
                     and no pre-registered client_id is configured"
                );
                return ContextServerState::Error {
                    configuration,
                    server,
                    error: "Authorization server supports neither CIMD nor DCR. \
                            Configure a pre-registered client_id in your settings \
                            under the \"oauth\" key."
                        .into(),
                };
            }

            log::info!(
                "{id} requires OAuth authorization (auth server: {})",
                discovery.auth_server_metadata.issuer,
            );
            ContextServerState::AuthRequired {
                server,
                configuration,
                discovery: Arc::new(discovery),
            }
        }
        Err(discovery_err) => {
            log::error!("{id} OAuth discovery failed: {discovery_err}");
            ContextServerState::Error {
                configuration,
                server,
                error: format!("OAuth discovery failed: {discovery_err}").into(),
            }
        }
    }
}

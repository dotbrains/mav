use super::*;

#[derive(Deserialize, RegisterSetting)]
pub struct ClientSettings {
    pub server_url: String,
    /// Overrides the key used to store credentials in the system keychain.
    /// Defaults to `server_url` when unset.
    ///
    /// Useful when running multiple Mav instances side by side without them
    /// overwriting each other's keychain entries.
    ///
    /// Note: changing this after signing in will require signing in again, as
    /// existing credentials are stored under the old key.
    pub credentials_url: Option<String>,
}

impl Settings for ClientSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        if let Some(server_url) = &*MAV_SERVER_URL {
            return Self {
                server_url: server_url.clone(),
                credentials_url: content.credentials_url.clone(),
            };
        }
        Self {
            server_url: content.server_url.clone().unwrap(),
            credentials_url: content.credentials_url.clone(),
        }
    }
}

#[derive(Deserialize, Default, RegisterSetting)]
pub struct ProxySettings {
    pub proxy: Option<String>,
}

impl ProxySettings {
    pub fn proxy_url(&self) -> Option<Url> {
        self.proxy
            .as_deref()
            .map(str::trim)
            .filter(|input| !input.is_empty())
            .and_then(|input| {
                input
                    .parse::<Url>()
                    .inspect_err(|e| log::error!("Error parsing proxy settings: {}", e))
                    .ok()
            })
            .or_else(read_proxy_from_env)
    }
}

impl Settings for ProxySettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self {
            proxy: content
                .proxy
                .as_deref()
                .map(str::trim)
                .filter(|proxy| !proxy.is_empty())
                .map(ToOwned::to_owned),
        }
    }
}

pub fn init(client: &Arc<Client>, cx: &mut App) {
    let client = Arc::downgrade(client);
    cx.on_action({
        let client = client.clone();
        move |_: &SignIn, cx| {
            if let Some(client) = client.upgrade() {
                cx.spawn(async move |cx| client.sign_in_with_optional_connect(true, cx).await)
                    .detach_and_log_err(cx);
            }
        }
    })
    .on_action({
        let client = client.clone();
        move |_: &SignOut, cx| {
            if let Some(client) = client.upgrade() {
                cx.spawn(async move |cx| {
                    client.sign_out(cx).await;
                })
                .detach();
            }
        }
    })
    .on_action({
        let client = client;
        move |_: &Reconnect, cx| {
            if let Some(client) = client.upgrade() {
                cx.spawn(async move |cx| {
                    client.reconnect(cx);
                })
                .detach();
            }
        }
    });
}
#[derive(Copy, Clone, Deserialize, Debug, RegisterSetting)]
pub struct TelemetrySettings {
    pub diagnostics: bool,
    pub metrics: bool,
    pub anthropic_retention: bool,
}

impl settings::Settings for TelemetrySettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let telemetry = content.telemetry.as_ref().unwrap();
        Self {
            diagnostics: telemetry.diagnostics.unwrap(),
            metrics: telemetry.metrics.unwrap(),
            anthropic_retention: telemetry.anthropic_retention.unwrap(),
        }
    }
}

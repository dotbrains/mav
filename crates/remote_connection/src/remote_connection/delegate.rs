use std::path::PathBuf;

use anyhow::{Context as _, Result};
use askpass::EncryptedPassword;
use auto_update::AutoUpdater;
use gpui::{AnyWindowHandle, AsyncApp, Task, WeakEntity};
use release_channel::ReleaseChannel;
use remote::RemotePlatform;
use semver::Version;

use crate::RemoteConnectionPrompt;

#[derive(Clone)]
pub struct RemoteClientDelegate {
    window: AnyWindowHandle,
    ui: WeakEntity<RemoteConnectionPrompt>,
    known_password: Option<EncryptedPassword>,
}

impl RemoteClientDelegate {
    pub fn new(
        window: AnyWindowHandle,
        ui: WeakEntity<RemoteConnectionPrompt>,
        known_password: Option<EncryptedPassword>,
    ) -> Self {
        Self {
            window,
            ui,
            known_password,
        }
    }

    fn update_status(&self, status: Option<&str>, cx: &mut AsyncApp) {
        cx.update(|cx| {
            self.ui
                .update(cx, |modal, cx| {
                    modal.set_status(status.map(|s| s.to_string()), cx);
                })
                .ok()
        });
    }
}

impl remote::RemoteClientDelegate for RemoteClientDelegate {
    fn ask_password(
        &self,
        prompt: String,
        tx: futures::channel::oneshot::Sender<EncryptedPassword>,
        cx: &mut AsyncApp,
    ) {
        let mut known_password = self.known_password.clone();
        if let Some(password) = known_password.take() {
            tx.send(password).ok();
        } else {
            self.window
                .update(cx, |_, window, cx| {
                    self.ui.update(cx, |modal, cx| {
                        modal.set_prompt(prompt, tx, window, cx);
                    })
                })
                .ok();
        }
    }

    fn set_status(&self, status: Option<&str>, cx: &mut AsyncApp) {
        self.update_status(status, cx)
    }

    fn download_server_binary_locally(
        &self,
        platform: RemotePlatform,
        release_channel: ReleaseChannel,
        version: Option<Version>,
        cx: &mut AsyncApp,
    ) -> Task<anyhow::Result<PathBuf>> {
        let this = self.clone();
        cx.spawn(async move |cx| {
            AutoUpdater::download_remote_server_release(
                release_channel,
                version.clone(),
                platform.os.as_str(),
                platform.arch.as_str(),
                move |status, cx| this.set_status(Some(status), cx),
                cx,
            )
            .await
            .with_context(|| {
                format!(
                    "Downloading remote server binary (version: {}, os: {}, arch: {})",
                    version
                        .as_ref()
                        .map(|v| format!("{}", v))
                        .unwrap_or("unknown".to_string()),
                    platform.os,
                    platform.arch,
                )
            })
        })
    }

    fn get_download_url(
        &self,
        platform: RemotePlatform,
        release_channel: ReleaseChannel,
        version: Option<Version>,
        cx: &mut AsyncApp,
    ) -> Task<Result<Option<String>>> {
        cx.spawn(async move |cx| {
            AutoUpdater::get_remote_server_release_url(
                release_channel,
                version,
                platform.os.as_str(),
                platform.arch.as_str(),
                cx,
            )
            .await
        })
    }
}

/// Delegate for remote connections that reuse an existing pooled
/// connection. Password prompts are not expected because the SSH
/// transport is already established.
pub struct BackgroundRemoteClientDelegate;

impl remote::RemoteClientDelegate for BackgroundRemoteClientDelegate {
    fn ask_password(
        &self,
        prompt: String,
        _tx: futures::channel::oneshot::Sender<EncryptedPassword>,
        _cx: &mut AsyncApp,
    ) {
        log::warn!(
            "Pooled remote connection unexpectedly requires a password \
             (prompt: {prompt})"
        );
    }

    fn set_status(&self, _status: Option<&str>, _cx: &mut AsyncApp) {}

    fn download_server_binary_locally(
        &self,
        platform: RemotePlatform,
        release_channel: ReleaseChannel,
        version: Option<Version>,
        cx: &mut AsyncApp,
    ) -> Task<anyhow::Result<PathBuf>> {
        cx.spawn(async move |cx| {
            AutoUpdater::download_remote_server_release(
                release_channel,
                version.clone(),
                platform.os.as_str(),
                platform.arch.as_str(),
                |_status, _cx| {},
                cx,
            )
            .await
            .with_context(|| {
                format!(
                    "Downloading remote server binary (version: {}, os: {}, arch: {})",
                    version
                        .as_ref()
                        .map(|v| format!("{v}"))
                        .unwrap_or("unknown".to_string()),
                    platform.os,
                    platform.arch,
                )
            })
        })
    }

    fn get_download_url(
        &self,
        platform: RemotePlatform,
        release_channel: ReleaseChannel,
        version: Option<Version>,
        cx: &mut AsyncApp,
    ) -> Task<Result<Option<String>>> {
        cx.spawn(async move |cx| {
            AutoUpdater::get_remote_server_release_url(
                release_channel,
                version,
                platform.os.as_str(),
                platform.arch.as_str(),
                cx,
            )
            .await
        })
    }
}

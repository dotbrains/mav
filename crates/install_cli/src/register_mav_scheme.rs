use client::MAV_URL_SCHEME;
use gpui::{AsyncApp, actions};

actions!(
    cli,
    [
        /// Registers the mav:// URL scheme handler.
        RegisterMavScheme
    ]
);

pub async fn register_mav_scheme(cx: &AsyncApp) -> anyhow::Result<()> {
    cx.update(|cx| cx.register_url_scheme(MAV_URL_SCHEME)).await
}

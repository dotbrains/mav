use axum::headers::{Header, HeaderName};
use semver::Version;
use std::sync::OnceLock;

pub struct ProtocolVersion(pub(crate) u32);

impl Header for ProtocolVersion {
    fn name() -> &'static HeaderName {
        static MAV_PROTOCOL_VERSION: OnceLock<HeaderName> = OnceLock::new();
        MAV_PROTOCOL_VERSION.get_or_init(|| HeaderName::from_static("x-mav-protocol-version"))
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i axum::http::HeaderValue>,
    {
        let version = values
            .next()
            .ok_or_else(axum::headers::Error::invalid)?
            .to_str()
            .map_err(|_| axum::headers::Error::invalid())?
            .parse()
            .map_err(|_| axum::headers::Error::invalid())?;
        Ok(Self(version))
    }

    fn encode<E: Extend<axum::http::HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.to_string().parse().unwrap()]);
    }
}

pub struct AppVersionHeader(pub(crate) Version);

impl Header for AppVersionHeader {
    fn name() -> &'static HeaderName {
        static MAV_APP_VERSION: OnceLock<HeaderName> = OnceLock::new();
        MAV_APP_VERSION.get_or_init(|| HeaderName::from_static("x-mav-app-version"))
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i axum::http::HeaderValue>,
    {
        let version = values
            .next()
            .ok_or_else(axum::headers::Error::invalid)?
            .to_str()
            .map_err(|_| axum::headers::Error::invalid())?
            .parse()
            .map_err(|_| axum::headers::Error::invalid())?;
        Ok(Self(version))
    }

    fn encode<E: Extend<axum::http::HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.to_string().parse().unwrap()]);
    }
}

#[derive(Debug)]
pub struct ReleaseChannelHeader(pub(crate) String);

impl Header for ReleaseChannelHeader {
    fn name() -> &'static HeaderName {
        static MAV_RELEASE_CHANNEL: OnceLock<HeaderName> = OnceLock::new();
        MAV_RELEASE_CHANNEL.get_or_init(|| HeaderName::from_static("x-mav-release-channel"))
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i axum::http::HeaderValue>,
    {
        Ok(Self(
            values
                .next()
                .ok_or_else(axum::headers::Error::invalid)?
                .to_str()
                .map_err(|_| axum::headers::Error::invalid())?
                .to_owned(),
        ))
    }

    fn encode<E: Extend<axum::http::HeaderValue>>(&self, values: &mut E) {
        values.extend([self.0.parse().unwrap()]);
    }
}

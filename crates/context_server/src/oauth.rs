//! OAuth 2.0 authentication for MCP servers using the Authorization Code +
//! PKCE flow, per the MCP spec's OAuth profile.
//!
//! The flow is split into two phases:
//!
//! 1. **Discovery** ([`discover`]) fetches Protected Resource Metadata and
//!    Authorization Server Metadata. This can happen early (e.g. on a 401
//!    during server startup) because it doesn't need the redirect URI yet.
//!
//! 2. **Client registration** ([`resolve_client_registration`]) is separate
//!    because DCR requires the actual loopback redirect URI, which includes an
//!    ephemeral port that only exists once the callback server has started.
//!
//! After authentication, the full state is captured in [`OAuthSession`] which
//! is persisted to the keychain. On next startup, the stored session feeds
//! directly into [`McpOAuthTokenProvider`], giving a refresh-capable provider
//! without requiring another browser flow.

use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use base64::Engine as _;
use futures::AsyncReadExt as _;
use futures::FutureExt as _;
use futures::channel::mpsc;
use futures::future::BoxFuture;
use http_client::{AsyncBody, HttpClient, Request};
use parking_lot::Mutex as SyncMutex;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use std::sync::Arc;
use std::time::{Duration, SystemTime};
use url::Url;

/// The CIMD URL where Mav's OAuth client metadata document is hosted.
mod callback;
mod dcr;
mod dcr_client;
mod discovery;
mod json;
mod metadata_urls;
mod pkce;
mod provider;
mod registration;
mod token;
mod token_client;
mod types;
mod validation;
mod www_authenticate;

pub use callback::*;
pub use dcr::*;
pub use dcr_client::*;
pub use discovery::*;
pub use metadata_urls::*;
pub use pkce::*;
pub use provider::*;
pub use registration::*;
pub use token::*;
pub use token_client::*;
pub use types::*;
pub use www_authenticate::*;

use json::*;
pub(crate) use validation::*;

#[cfg(test)]
mod tests;

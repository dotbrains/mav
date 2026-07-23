use crate::{
    Event, ExtensionIndex, ExtensionIndexEntry, ExtensionIndexLanguageEntry,
    ExtensionIndexThemeEntry, ExtensionManifest, ExtensionStore, GrammarManifestEntry,
    RELOAD_DEBOUNCE_DURATION, SchemaVersion,
};
use async_compression::futures::bufread::GzipEncoder;
use collections::{BTreeMap, HashSet};
use extension::ExtensionHostProxy;
use fs::{FakeFs, Fs, RealFs};
use futures::{AsyncReadExt, FutureExt, StreamExt, io::BufReader};
use gpui::{AppContext as _, BackgroundExecutor, TaskExt, TestAppContext};
use http_client::{FakeHttpClient, Response};
use language::{BinaryStatus, LanguageMatcher, LanguageName, LanguageRegistry};
use language_extension::LspAccess;
use lsp::LanguageServerName;
use node_runtime::NodeRuntime;
use parking_lot::Mutex;
use project::{DEFAULT_COMPLETION_CONTEXT, Project};
use release_channel::AppVersion;
use reqwest_client::ReqwestClient;
use serde_json::json;
use settings::SettingsStore;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};
use theme::ThemeRegistry;
use util::{rel_path::rel_path_buf, test::TempTree};

#[cfg(test)]
#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

mod remote_sync;
mod store_reload;
mod test_extension;
mod test_support;

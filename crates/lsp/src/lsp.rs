mod input_handler;

pub use lsp_types::request::*;
pub use lsp_types::*;

use anyhow::{Context as _, Result, anyhow};
use collections::{BTreeMap, HashMap};
use futures::{
    AsyncRead, AsyncWrite, Future, FutureExt,
    channel::oneshot::{self, Canceled},
    future::{self, Either},
    io::BufWriter,
    select,
};
use gpui::{App, AppContext as _, AsyncApp, BackgroundExecutor, SharedString, Task};
use notification::DidChangeWorkspaceFolders;
use parking_lot::{Mutex, RwLock};
use postage::{barrier, prelude::Stream};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json, value::RawValue};
use smol::{
    channel,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
};
use util::command::{Child, Stdio};

use std::path::Path;
use std::{
    any::TypeId,
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fmt,
    io::Write,
    ops::DerefMut,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicI32, Ordering::SeqCst},
    },
    task::Poll,
    time::{Duration, Instant},
};
use util::{ConnectionResult, ResultExt, TryFutureExt, redact};

#[path = "lsp/display.rs"]
mod display;
#[cfg(any(test, feature = "test-support"))]
#[path = "lsp/fake.rs"]
mod fake;
#[path = "lsp/handlers.rs"]
mod handlers;
#[path = "lsp/initialize.rs"]
mod initialize;
#[path = "lsp/process.rs"]
mod process;
#[path = "lsp/protocol.rs"]
mod protocol;
#[path = "lsp/requests.rs"]
mod requests;
#[cfg(test)]
#[path = "lsp/tests.rs"]
mod tests;
#[path = "lsp/workspace.rs"]
mod workspace;

#[cfg(any(test, feature = "test-support"))]
pub use fake::FakeLanguageServer;
pub use protocol::*;

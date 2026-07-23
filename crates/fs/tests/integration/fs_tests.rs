mod fake_git_repo_tests;

use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use futures::{FutureExt, StreamExt};

use fs::*;
use gpui::{BackgroundExecutor, TestAppContext};
use serde_json::json;
use tempfile::TempDir;
use util::path;

mod copy_recursive;
mod fake_fs;
mod realfs_io;
mod trash_restore;
mod watch;

#[path = "tests/common.rs"]
mod common;
#[path = "tests/drag_drop.rs"]
mod drag_drop;
#[path = "tests/rename.rs"]
mod rename;
#[path = "tests/working_directory.rs"]
mod working_directory;

use super::*;
use gpui::{TestAppContext, VisualTestContext};
use project::{Entry, Project, ProjectPath, Worktree};
use remote::RemoteClient;
use std::path::{Path, PathBuf};
use util::paths::PathStyle;
use util::rel_path::RelPath;
use workspace::item::test::{TestItem, TestProjectItem};
use workspace::{AppState, MultiWorkspace, SelectedEntry};

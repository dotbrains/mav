use std::{
    path::{self, Path, PathBuf},
    sync::Arc,
};

use call::ActiveCall;
use client::RECEIVE_TIMEOUT;
use collections::HashMap;
use git::{
    Oid,
    repository::{CommitData, InitialGraphCommitData, RepoPath, Worktree as GitWorktree},
    status::{DiffStat, FileStatus, StatusCode, TrackedStatus},
};
use git_ui::git_graph::GitGraph;
use git_ui::{git_panel::GitPanel, project_diff::ProjectDiff};
use gpui::{
    AppContext as _, BackgroundExecutor, Entity, IntoElement as _, SharedString, TestAppContext,
    VisualContext as _, VisualTestContext, point, px, size,
};
use project::{
    ProjectPath,
    git_store::{CommitDataState, Repository},
};
use rand::{SeedableRng, rngs::StdRng};
use serde_json::json;

use util::{path, rel_path::rel_path};
use workspace::{MultiWorkspace, Workspace};

use crate::TestServer;

mod branch_list_sync;
mod commit_data_batches;
mod diff_stat_sync;
mod graph_data_and_search;
mod linked_worktrees_sync;
mod project_diff;
mod remote_head_sha;
mod remote_worktrees;
mod root_repo_common_dir;
mod test_support;

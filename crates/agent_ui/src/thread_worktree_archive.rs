use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use anyhow::{Context as _, Result, anyhow};
use gpui::{App, AsyncApp, Entity, Task};
use project::{
    LocalProjectFlags, Project, WorktreeId,
    git_store::{Repository, resolve_git_worktree_to_main_repo, worktrees_directory_for_repo},
    project_settings::ProjectSettings,
};
use remote::{RemoteConnectionOptions, same_remote_connection_identity};
use settings::Settings;
use util::{ResultExt, paths::PathStyle};
use workspace::{AppState, MultiWorkspace, Workspace};

use crate::thread_metadata_store::{ArchivedGitWorktree, ThreadId, ThreadMetadataStore};

/// The plan for archiving a single git worktree root.
///
/// A thread can have multiple folder paths open, so there may be multiple
/// `RootPlan`s per archival operation. Each one captures everything needed to
/// persist the worktree's git state and then remove it from disk.
///
/// All fields are gathered synchronously by [`build_root_plan`] while the
/// worktree is still loaded in open projects. This is important because
/// workspace removal tears down project and repository entities, making
/// them unavailable for the later async persist/remove steps.
#[derive(Clone)]
pub struct RootPlan {
    /// Absolute path of the git worktree on disk.
    pub root_path: PathBuf,
    /// Absolute path to the main git repository this worktree is linked to.
    /// Used both for creating a git ref to prevent GC of WIP commits during
    /// [`persist_worktree_state`], and for `git worktree remove` during
    /// [`remove_root`].
    pub main_repo_path: PathBuf,
    /// Every open `Project` that has this worktree loaded, so they can all
    /// call `remove_worktree` and release it during [`remove_root`].
    /// Multiple projects can reference the same path when the user has the
    /// worktree open in more than one workspace.
    pub affected_projects: Vec<AffectedProject>,
    /// The `Repository` entity for this linked worktree, used to run git
    /// commands (create WIP commits, stage files, reset) during
    /// [`persist_worktree_state`].
    pub worktree_repo: Entity<Repository>,
    /// The branch the worktree was on, so it can be restored later.
    /// `None` if the worktree was in detached HEAD state.
    pub branch_name: Option<String>,
    /// Remote connection options for the project that owns this worktree,
    /// used to create temporary remote projects when the main repo isn't
    /// loaded in any open workspace.
    pub remote_connection: Option<RemoteConnectionOptions>,
    /// The creation time of the worktree's git metadata directory that was
    /// recorded when Mav created the worktree. [`remove_root`] re-stats the
    /// directory and refuses to delete anything if the time has changed,
    /// which means the worktree was recreated outside Mav.
    pub recorded_created_at: SystemTime,
}

/// A `Project` that references a worktree being archived, paired with the
/// `WorktreeId` it uses for that worktree.
///
/// The same worktree path can appear in multiple open workspaces/projects
/// (e.g. when the user has two windows open that both include the same
/// linked worktree). Each one needs to call `remove_worktree` and wait for
/// the release during [`remove_root`], otherwise the project would still
/// hold a reference to the directory and `git worktree remove` would fail.
#[derive(Clone)]
pub struct AffectedProject {
    pub project: Entity<Project>,
    pub worktree_id: WorktreeId,
}

fn archived_worktree_ref_name(id: i64) -> String {
    format!("refs/archived-worktrees/{}", id)
}

/// Resolves the Mav-managed worktrees base directory for a given repo.
///
/// This intentionally reads the *global* `git.worktree_directory` setting
/// rather than any project-local override, because Mav always uses the
/// global value when creating worktrees and the archive check must match.
fn worktrees_base_for_repo(
    main_repo_path: &Path,
    path_style: PathStyle,
    cx: &App,
) -> Option<PathBuf> {
    let setting = &ProjectSettings::get_global(cx).git.worktree_directory;
    worktrees_directory_for_repo(main_repo_path, setting, path_style).log_err()
}

/// Builds a [`RootPlan`] for archiving the git worktree at `path`.
///
/// This is a synchronous planning step that must run *before* any workspace
/// removal, because it needs live project and repository entities that are
/// torn down when a workspace is removed. It does three things:
///
/// 1. Finds every `Project` across all open workspaces that has this
///    worktree loaded (`affected_projects`).
/// 2. Looks for a `Repository` entity whose snapshot identifies this path
///    as a linked worktree (`worktree_repo`), which is needed for the git
///    operations in [`persist_worktree_state`].
/// 3. Determines the `main_repo_path` — the parent repo that owns this
///    linked worktree — needed for both git ref creation and
///    `git worktree remove`.
///
/// Returns `None` if the path is not a linked worktree (main worktrees
/// cannot be archived to disk) or if no open project has it loaded.
pub fn build_root_plan(
    path: &Path,
    remote_connection: Option<&RemoteConnectionOptions>,
    workspaces: &[Entity<Workspace>],
    cx: &App,
) -> Option<RootPlan> {
    let path = path.to_path_buf();

    let matches_target_connection = |project: &Entity<Project>, cx: &App| {
        same_remote_connection_identity(
            project.read(cx).remote_connection_options(cx).as_ref(),
            remote_connection,
        )
    };

    let affected_projects = workspaces
        .iter()
        .filter_map(|workspace| {
            let project = workspace.read(cx).project().clone();
            if !matches_target_connection(&project, cx) {
                return None;
            }
            let worktree = project
                .read(cx)
                .visible_worktrees(cx)
                .find(|worktree| worktree.read(cx).abs_path().as_ref() == path.as_path())?;
            let worktree_id = worktree.read(cx).id();
            Some(AffectedProject {
                project,
                worktree_id,
            })
        })
        .collect::<Vec<_>>();

    if affected_projects.is_empty() {
        return None;
    }

    let linked_repo = workspaces
        .iter()
        .filter(|workspace| matches_target_connection(workspace.read(cx).project(), cx))
        .flat_map(|workspace| {
            workspace
                .read(cx)
                .project()
                .read(cx)
                .repositories(cx)
                .values()
                .cloned()
                .collect::<Vec<_>>()
        })
        .find_map(|repo| {
            let snapshot = repo.read(cx).snapshot();
            (snapshot.is_linked_worktree()
                && snapshot.work_directory_abs_path.as_ref() == path.as_path())
            .then_some((snapshot, repo))
        });

    // Only linked worktrees can be archived to disk via `git worktree remove`.
    // Main worktrees must be left alone — git refuses to remove them.
    let (linked_snapshot, repo) = linked_repo?;
    let main_repo_path = linked_snapshot.main_worktree_abs_path()?.to_path_buf();

    // Only archive worktrees that live inside the Mav-managed worktrees
    // directory (configured via `git.worktree_directory`). Worktrees the
    // user created outside that directory should be left untouched.
    let worktrees_base = worktrees_base_for_repo(&main_repo_path, linked_snapshot.path_style, cx)?;
    if !path.starts_with(&worktrees_base) {
        return None;
    }

    // Only archive worktrees that Mav explicitly created. The directory
    // check above constrains paths, but the database record is what
    // distinguishes a Mav-created worktree from one the user manually
    // created under the same directory layout. The recorded creation time
    // is re-verified against the filesystem in [`remove_root`] before
    // anything is deleted.
    let recorded_created_at =
        git_ui::created_worktrees::recorded_created_at(&path, remote_connection, cx)?;

    let branch_name = linked_snapshot
        .branch
        .as_ref()
        .map(|branch| branch.name().to_string());

    Some(RootPlan {
        root_path: path,
        main_repo_path,
        affected_projects,
        worktree_repo: repo,
        branch_name,
        remote_connection: remote_connection.cloned(),
        recorded_created_at,
    })
}

/// Removes a worktree from all affected projects and deletes it from disk
/// via `git worktree remove`.
///
/// This is the destructive counterpart to [`persist_worktree_state`]. It
/// first detaches the worktree from every [`AffectedProject`], waits for
/// each project to fully release it, then asks the main repository to
/// delete the worktree directory. If the git removal fails, the worktree
/// is re-added to each project via [`rollback_root`].
mod cleanup;
mod persist;
mod remove;
mod restore;
mod workspace_lookup;

#[cfg(test)]
mod custom_directory_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod remove_tests;
#[cfg(test)]
mod test_support;

pub use cleanup::{cleanup_archived_worktree_record, cleanup_thread_archived_worktrees};
pub use persist::{persist_worktree_state, rollback_persist};
pub use remove::remove_root;
pub use restore::restore_worktree_via_git;
pub use workspace_lookup::{all_open_workspaces, workspaces_for_archive};

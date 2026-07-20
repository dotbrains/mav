pub(crate) use std::{cell::RefCell, path::PathBuf, rc::Rc};

pub(crate) use collections::HashSet;
pub(crate) use gpui::{Entity, TestAppContext};
pub(crate) use serde_json::json;
pub(crate) use settings::SettingsStore;
pub(crate) use util::path;

pub(crate) use crate::{FakeFs, Project};

pub(crate) use project::{trusted_worktrees::*, worktree_store::WorktreeStore};

pub(crate) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        if cx.try_global::<SettingsStore>().is_none() {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        }
        if cx.try_global::<TrustedWorktrees>().is_some() {
            cx.remove_global::<TrustedWorktrees>();
        }
    });
}

pub(crate) fn init_trust_global(
    worktree_store: Entity<WorktreeStore>,
    cx: &mut TestAppContext,
) -> Entity<TrustedWorktreesStore> {
    cx.update(|cx| {
        init(DbTrustedPaths::default(), cx);
        track_worktree_trust(worktree_store, None, None, None, cx);
        TrustedWorktrees::try_get_global(cx).expect("global should be set")
    })
}

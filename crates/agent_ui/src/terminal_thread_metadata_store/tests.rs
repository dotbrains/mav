use std::path::Path;

use gpui::TestAppContext;

use super::*;

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        TerminalThreadMetadataStore::init_global(cx);
    });
    cx.run_until_parked();
}

fn metadata(title: &str, worktree_paths: WorktreePaths) -> TerminalThreadMetadata {
    let now = Utc::now();
    TerminalThreadMetadata {
        terminal_id: TerminalId::new(),
        title: SharedString::from(title.to_string()),
        custom_title: None,
        created_at: now,
        worktree_paths,
        remote_connection: None,
        working_directory: None,
    }
}

#[test]
fn test_terminal_title_prefix_preserves_non_alphanumeric_prefixes() {
    assert_eq!(terminal_title_prefix("✳ Thinking"), Some("✳ "));
    assert_eq!(terminal_title_prefix(">>>   Thinking"), Some(">>>   "));
    assert_eq!(terminal_title_prefix("⠋ Running"), Some("⠋ "));
    assert_eq!(terminal_title_prefix("* Claude"), Some("* "));
    assert_eq!(terminal_title_prefix("✳Thinking"), None);
    assert_eq!(terminal_title_prefix("Thinking"), None);
    assert_eq!(terminal_title_prefix(" Thinking"), None);
    assert_eq!(terminal_title_prefix("✳"), None);
    assert_eq!(terminal_title_prefix("v1 Running"), None);
}

#[test]
fn test_terminal_thread_display_title_combines_raw_and_custom_titles() {
    let mut metadata = metadata(
        "⠋ Thinking",
        WorktreePaths::from_folder_paths(&PathList::default()),
    );
    metadata.custom_title = Some("Fix bug".into());
    assert_eq!(metadata.display_title().as_ref(), "⠋ Fix bug");

    metadata.title = "Thinking".into();
    assert_eq!(metadata.display_title().as_ref(), "Fix bug");
}

#[gpui::test]
async fn test_change_worktree_paths_reindexes_terminal_metadata(cx: &mut TestAppContext) {
    init_test(cx);

    let old_main_paths = PathList::new(&[Path::new("/repo")]);
    let old_folder_paths = PathList::new(&[Path::new("/repo-feature")]);
    let new_main_path = Path::new("/repo");
    let new_folder_path = Path::new("/repo-feature-renamed");
    let new_folder_paths = PathList::new(&[new_folder_path]);
    let metadata = metadata(
        "Dev Server",
        WorktreePaths::from_path_lists(old_main_paths.clone(), old_folder_paths.clone()).unwrap(),
    );
    let terminal_id = metadata.terminal_id;

    cx.update(|cx| {
        TerminalThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });

    cx.update(|cx| {
        TerminalThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.change_worktree_paths(
                &old_folder_paths,
                None,
                |paths| {
                    paths.add_path(new_main_path, new_folder_path);
                    paths.remove_folder_path(Path::new("/repo-feature"));
                },
                cx,
            );
        });
    });

    cx.update(|cx| {
        let store = TerminalThreadMetadataStore::global(cx);
        let store = store.read(cx);
        assert!(
            store
                .entries_for_path(&old_folder_paths, None)
                .next()
                .is_none()
        );
        assert_eq!(
            store
                .entries_for_path(&new_folder_paths, None)
                .map(|entry| entry.terminal_id)
                .collect::<Vec<_>>(),
            vec![terminal_id]
        );
        assert_eq!(
            store
                .entry(terminal_id)
                .unwrap()
                .main_worktree_paths()
                .paths(),
            old_main_paths.paths()
        );
    });
}

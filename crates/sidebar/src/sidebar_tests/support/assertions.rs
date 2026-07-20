use super::*;

pub(super) fn assert_active_thread(sidebar: &Sidebar, session_id: &acp::SessionId, msg: &str) {
    let active = sidebar.active_entry.as_ref();
    let matches = active.is_some_and(|entry| {
        matches!(entry, ActiveEntry::Thread { session_id: Some(active_session_id), .. } if active_session_id == session_id)
            || sidebar.contents.entries.iter().any(|list_entry| {
                matches!(list_entry, ListEntry::Thread(t)
                    if t.metadata.session_id.as_ref() == Some(session_id)
                        && entry.matches_entry(list_entry))
            })
    });
    assert!(
        matches,
        "{msg}: expected active_entry for session {session_id:?}, got {:?}",
        active,
    );
}

#[track_caller]
pub(super) fn is_active_session(sidebar: &Sidebar, session_id: &acp::SessionId) -> bool {
    let thread_id = sidebar
        .contents
        .entries
        .iter()
        .find_map(|entry| match entry {
            ListEntry::Thread(t) if t.metadata.session_id.as_ref() == Some(session_id) => {
                Some(t.metadata.thread_id)
            }
            _ => None,
        });
    match thread_id {
        Some(tid) => {
            matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { thread_id, .. }) if *thread_id == tid)
        }
        // Thread not in sidebar entries — can't confirm it's active.
        None => false,
    }
}

#[track_caller]
pub(super) fn assert_active_draft(sidebar: &Sidebar, workspace: &Entity<Workspace>, msg: &str) {
    assert!(
        matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { workspace: ws, .. }) if ws == workspace),
        "{msg}: expected active_entry to be Draft for workspace {:?}, got {:?}",
        workspace.entity_id(),
        sidebar.active_entry,
    );
}

pub(super) fn has_thread_entry(sidebar: &Sidebar, session_id: &acp::SessionId) -> bool {
    sidebar
        .contents
        .entries
        .iter()
        .any(|entry| matches!(entry, ListEntry::Thread(t) if t.metadata.session_id.as_ref() == Some(session_id)))
}

#[track_caller]
pub(super) fn assert_project_header_has_threads(
    sidebar: &Entity<Sidebar>,
    project_name: &str,
    expected_has_threads: bool,
    cx: &mut gpui::VisualTestContext,
) {
    sidebar.read_with(cx, |sidebar, _cx| {
        let has_threads = sidebar.contents.entries.iter().find_map(|entry| {
            if let ListEntry::ProjectHeader {
                label, has_threads, ..
            } = entry
                && label.as_ref() == project_name
            {
                Some(*has_threads)
            } else {
                None
            }
        });

        assert_eq!(
            has_threads,
            Some(expected_has_threads),
            "expected project header `{project_name}` to have has_threads={expected_has_threads}, got {has_threads:?}"
        );
    });
}

#[track_caller]
pub(super) fn assert_remote_project_integration_sidebar_state(
    sidebar: &mut Sidebar,
    main_thread_id: &acp::SessionId,
    remote_thread_id: &acp::SessionId,
) {
    let mut project_headers = sidebar.contents.entries.iter().filter_map(|entry| {
        if let ListEntry::ProjectHeader { label, .. } = entry {
            Some(label.as_ref())
        } else {
            None
        }
    });

    let Some(project_header) = project_headers.next() else {
        panic!("expected exactly one sidebar project header named `project`, found none");
    };
    assert_eq!(
        project_header, "project",
        "expected the only sidebar project header to be `project`"
    );
    if let Some(unexpected_header) = project_headers.next() {
        panic!(
            "expected exactly one sidebar project header named `project`, found extra header `{unexpected_header}`"
        );
    }

    let mut saw_main_thread = false;
    let mut saw_remote_thread = false;
    for entry in &sidebar.contents.entries {
        match entry {
            ListEntry::ProjectHeader { label, .. } => {
                assert_eq!(
                    label.as_ref(),
                    "project",
                    "expected the only sidebar project header to be `project`"
                );
            }
            ListEntry::Thread(thread)
                if thread.metadata.session_id.as_ref() == Some(main_thread_id) =>
            {
                saw_main_thread = true;
            }
            ListEntry::Thread(thread)
                if thread.metadata.session_id.as_ref() == Some(remote_thread_id) =>
            {
                saw_remote_thread = true;
            }
            ListEntry::Thread(thread) => {
                let title = thread.metadata.display_title();
                panic!(
                    "unexpected sidebar thread while simulating remote project integration flicker: title=`{}`",
                    title
                );
            }
            ListEntry::Terminal(terminal) => {
                panic!(
                    "unexpected sidebar terminal while simulating remote project integration flicker: title=`{}`",
                    terminal.metadata.title
                );
            }
        }
    }

    assert!(
        saw_main_thread,
        "expected the sidebar to keep showing `Main Thread` under `project`"
    );
    assert!(
        saw_remote_thread,
        "expected the sidebar to keep showing `Worktree Thread` under `project`"
    );
}

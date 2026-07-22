use super::*;

fn expected_drop_text(paths: &[PathBuf]) -> String {
    let mut text = String::new();
    for path in paths {
        text.push(' ');
        text.push_str(&format!("{path:?}"));
    }
    text.push(' ');
    text
}

fn assert_drop_writes_to_terminal(
    pane: &Entity<Pane>,
    terminal_view_index: usize,
    terminal: &Entity<Terminal>,
    dropped: &dyn Any,
    expected_text: &str,
    window: &mut Window,
    cx: &mut Context<MultiWorkspace>,
) {
    let _ = terminal.update(cx, |terminal, _| terminal.take_input_log());

    let handled = pane.update(cx, |pane, cx| {
        pane.item_for_index(terminal_view_index)
            .unwrap()
            .handle_drop(pane, dropped, window, cx)
    });
    assert!(handled, "handle_drop should return true for {:?}", dropped);

    let mut input_log = terminal.update(cx, |terminal, _| terminal.take_input_log());
    assert_eq!(input_log.len(), 1, "expected exactly one write to terminal");
    let written =
        String::from_utf8(input_log.remove(0)).expect("terminal write should be valid UTF-8");
    assert_eq!(written, expected_text);
}

// DEC private mode 1049: a program writes this to enter the alternate screen buffer.
const ENTER_ALT_SCREEN: &[u8] = b"\x1b[?1049h";

// CSI `1;2A` = cursor-up with the xterm Shift modifier (`1 + 1` for Shift).
const SHIFT_UP_ESCAPE: &[u8] = b"\x1b[1;2A";

#[gpui::test]
async fn edit_menu_copy_and_paste_are_available_when_terminal_is_focused(cx: &mut TestAppContext) {
    let (project, _workspace, window_handle) = init_test_with_window(cx).await;
    let (_pane, terminal, _terminal_view) =
        add_display_only_terminal(&project, window_handle, true, cx);

    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);
    cx.update(|window, cx| {
        let _ = window.draw(cx);
        assert!(window.is_action_available(&editor::actions::Copy, cx));
        assert!(window.is_action_available(&editor::actions::Paste, cx));

        cx.write_to_clipboard(gpui::ClipboardItem::new_string("foo".to_string()));
        terminal.update(cx, |terminal, _| terminal.take_input_log());
        window.dispatch_action(Box::new(editor::actions::Paste), cx);
    });
    cx.run_until_parked();

    cx.update(|_, cx| {
        let input_log = terminal.update(cx, |terminal, _| terminal.take_input_log());
        assert_eq!(input_log, vec![b"foo".to_vec()]);
    });
}

#[gpui::test]
async fn shift_up_scrolls_history_in_normal_screen(cx: &mut TestAppContext) {
    let (project, _workspace, window_handle) = init_test_with_window(cx).await;
    cx.update(load_default_keymap);
    let (_pane, terminal, _terminal_view) =
        add_display_only_terminal(&project, window_handle, true, cx);

    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.run_until_parked();

    let output = (0..200)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    cx.update(|window, cx| {
        terminal.update(cx, |terminal, cx| {
            terminal.write_output(output.as_bytes(), cx);
            terminal.sync(window, cx);
        });
    });
    terminal.read_with(&cx, |terminal, _| {
        assert!(!terminal.last_content.mode.contains(Modes::ALT_SCREEN));
        assert_eq!(terminal.last_content.display_offset, 0);
    });

    cx.simulate_keystrokes("shift-up");
    cx.update(|window, cx| {
        terminal.update(cx, |terminal, cx| terminal.sync(window, cx));
    });

    assert_eq!(
        terminal.read_with(&cx, |terminal, _| terminal.last_content.display_offset),
        1,
        "shift-up should scroll terminal history in the normal screen",
    );
    assert!(
        terminal
            .update(&mut cx, |terminal, _| terminal.take_input_log())
            .is_empty(),
        "shift-up in the normal screen should not be forwarded to the shell",
    );
}

#[gpui::test]
async fn shift_up_is_forwarded_to_program_in_alt_screen(cx: &mut TestAppContext) {
    let (project, _workspace, window_handle) = init_test_with_window(cx).await;
    cx.update(load_default_keymap);
    let (_pane, terminal, _terminal_view) =
        add_display_only_terminal(&project, window_handle, true, cx);

    let mut cx = VisualTestContext::from_window(window_handle.into(), cx);
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        terminal.update(cx, |terminal, cx| {
            terminal.write_output(ENTER_ALT_SCREEN, cx);
            terminal.sync(window, cx);
        });
    });
    terminal.read_with(&cx, |terminal, _| {
        assert!(terminal.last_content.mode.contains(Modes::ALT_SCREEN));
    });

    cx.simulate_keystrokes("shift-up");
    assert_eq!(
        terminal.update(&mut cx, |terminal, _| terminal.take_input_log()),
        vec![SHIFT_UP_ESCAPE.to_vec()],
        "shift-up should be forwarded to the program in the alternate screen",
    );
}

/// Creates a worktree with 1 file: /root.txt
pub async fn init_test(cx: &mut TestAppContext) -> (Entity<Project>, Entity<Workspace>) {
    let (project, workspace, _) = init_test_with_window(cx).await;
    (project, workspace)
}

fn load_default_keymap(cx: &mut App) {
    cx.bind_keys(
        settings::KeymapFile::load_asset_allow_partial_failure(settings::DEFAULT_KEYMAP_PATH, cx)
            .unwrap(),
    );
}

fn add_display_only_terminal(
    project: &Entity<Project>,
    window_handle: gpui::WindowHandle<MultiWorkspace>,
    focus: bool,
    cx: &mut TestAppContext,
) -> (Entity<Pane>, Entity<Terminal>, Entity<TerminalView>) {
    let project = project.clone();
    window_handle
        .update(cx, |multi_workspace, window, cx| {
            let workspace = multi_workspace.workspace().clone();
            let active_pane = workspace.read(cx).active_pane().clone();

            let terminal = cx.new(|cx| {
                terminal::TerminalBuilder::new_display_only(
                    CursorShape::default(),
                    terminal::terminal_settings::AlternateScroll::On,
                    None,
                    0,
                    cx.background_executor(),
                    PathStyle::local(),
                )
                .subscribe(cx)
            });
            let terminal_view = cx.new(|cx| {
                TerminalView::new(
                    terminal.clone(),
                    workspace.downgrade(),
                    None,
                    project.downgrade(),
                    window,
                    cx,
                )
            });

            active_pane.update(cx, |pane, cx| {
                pane.add_item(
                    Box::new(terminal_view.clone()),
                    true,
                    false,
                    None,
                    window,
                    cx,
                );
            });

            if focus {
                let focus_handle = terminal_view.read(cx).focus_handle.clone();
                focus_handle.focus(window, cx);
            }

            (active_pane, terminal, terminal_view)
        })
        .unwrap()
}

/// Creates a worktree with 1 file /root.txt and returns the project, workspace, and window handle.
async fn init_test_with_window(
    cx: &mut TestAppContext,
) -> (
    Entity<Project>,
    Entity<Workspace>,
    gpui::WindowHandle<MultiWorkspace>,
) {
    let params = cx.update(AppState::test);
    cx.update(|cx| {
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });

    let project = Project::test(params.fs.clone(), [], cx).await;
    let window_handle =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    (project, workspace, window_handle)
}

async fn init_remote_test(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) -> (Entity<Project>, Entity<Workspace>) {
    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let params = cx.update(AppState::test);
    let (opts, server_session, connect_guard) = RemoteClient::fake_server(cx, server_cx);
    let ping_handler = server_cx.new(|_| ());
    server_session.add_request_handler::<rpc::proto::Ping, _, _, _>(
        ping_handler.downgrade(),
        |_entity, _envelope, _cx| async { Ok(rpc::proto::Ack {}) },
    );
    drop(connect_guard);

    let remote_client = RemoteClient::connect_mock(opts, cx).await;
    let project = cx.update(|cx| {
        Project::remote(
            remote_client,
            params.client.clone(),
            params.node_runtime.clone(),
            params.user_store.clone(),
            params.languages.clone(),
            params.fs.clone(),
            false,
            cx,
        )
    });

    let window_handle = cx.add_window({
        let params = params.clone();
        let project_for_workspace = project.clone();
        move |window, cx| {
            window.activate_window();
            let workspace = cx.new(|cx| {
                Workspace::new(
                    None,
                    project_for_workspace.clone(),
                    params.clone(),
                    window,
                    cx,
                )
            });
            MultiWorkspace::new(workspace, window, cx)
        }
    });
    let workspace = window_handle
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    (project, workspace)
}

/// Creates a file in the given worktree and returns its entry.
async fn create_file_in_worktree(
    worktree: Entity<Worktree>,
    relative_path: impl AsRef<Path>,
    cx: &mut TestAppContext,
) -> Entry {
    cx.update(|cx| {
        worktree.update(cx, |worktree, cx| {
            worktree.create_entry(
                RelPath::new(relative_path.as_ref(), PathStyle::local())
                    .unwrap()
                    .as_ref()
                    .into(),
                false,
                None,
                cx,
            )
        })
    })
    .await
    .unwrap()
    .into_included()
    .unwrap()
}

/// Creates a worktree with 1 folder: /root{suffix}/
async fn create_folder_wt(
    project: Entity<Project>,
    path: impl AsRef<Path>,
    cx: &mut TestAppContext,
) -> (Entity<Worktree>, Entry) {
    create_wt(project, true, path, cx).await
}

/// Creates a worktree with 1 file: /root{suffix}.txt
async fn create_file_wt(
    project: Entity<Project>,
    path: impl AsRef<Path>,
    cx: &mut TestAppContext,
) -> (Entity<Worktree>, Entry) {
    create_wt(project, false, path, cx).await
}

async fn create_wt(
    project: Entity<Project>,
    is_dir: bool,
    path: impl AsRef<Path>,
    cx: &mut TestAppContext,
) -> (Entity<Worktree>, Entry) {
    let (wt, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path, true, cx)
        })
        .await
        .unwrap();

    let entry = cx
        .update(|cx| {
            wt.update(cx, |wt, cx| {
                wt.create_entry(RelPath::empty_arc(), is_dir, None, cx)
            })
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    (wt, entry)
}

pub fn insert_active_entry_for(
    wt: Entity<Worktree>,
    entry: Entry,
    project: Entity<Project>,
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let p = ProjectPath {
            worktree_id: wt.read(cx).id(),
            path: entry.path,
        };
        project.update(cx, |project, cx| project.set_active_path(Some(p), cx));
    });
}

use super::*;

#[track_caller]
fn assert_active_item(
    workspace: &mut Workspace,
    expected_path: &str,
    expected_text: &str,
    cx: &mut Context<Workspace>,
) {
    let active_editor = workspace.active_item_as::<Editor>(cx).unwrap();

    let buffer = active_editor
        .read(cx)
        .buffer()
        .read(cx)
        .as_singleton()
        .unwrap();

    let text = buffer.read(cx).text();
    let file = buffer.read(cx).file().unwrap();
    let file_path = file.as_local().unwrap().abs_path(cx);

    assert_eq!(text, expected_text);
    assert_eq!(file_path, Path::new(expected_path));
}

#[gpui::test]
async fn test_command_gf(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Assert base state, that we're in /root/dir/file.rs
    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file.rs"), "", cx);
    });

    // Insert a new file
    let fs = cx.workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());
    fs.as_fake()
        .insert_file(
            path!("/root/dir/file2.rs"),
            "This is file2.rs".as_bytes().to_vec(),
        )
        .await;
    fs.as_fake()
        .insert_file(
            path!("/root/dir/file3.rs"),
            "go to file3".as_bytes().to_vec(),
        )
        .await;

    // Put the path to the second file into the currently open buffer
    cx.set_state(indoc! {"go to fiˇle2.rs"}, Mode::Normal);

    // Go to file2.rs
    cx.simulate_keystrokes("g f");

    // We now have two items
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 2));
    cx.workspace(|workspace, _, cx| {
        assert_active_item(
            workspace,
            path!("/root/dir/file2.rs"),
            "This is file2.rs",
            cx,
        );
    });

    // Update editor to point to `file2.rs`
    cx.editor = cx.workspace(|workspace, _, cx| workspace.active_item_as::<Editor>(cx).unwrap());

    // Put the path to the third file into the currently open buffer,
    // but remove its suffix, because we want that lookup to happen automatically.
    cx.set_state(indoc! {"go to fiˇle3"}, Mode::Normal);

    // Go to file3.rs
    cx.simulate_keystrokes("g f");

    // We now have three items
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 3));
    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file3.rs"), "go to file3", cx);
    });
}

#[gpui::test]
async fn test_command_write_filename(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file.rs"), "", cx);
    });

    cx.simulate_keystrokes(": w space other.rs");
    cx.simulate_keystrokes("enter");

    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/other.rs"), "", cx);
    });

    cx.simulate_keystrokes(": w space dir/file.rs");
    cx.simulate_keystrokes("enter");

    cx.simulate_prompt_answer("Replace");
    cx.run_until_parked();

    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file.rs"), "", cx);
    });

    cx.simulate_keystrokes(": w ! space other.rs");
    cx.simulate_keystrokes("enter");

    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/other.rs"), "", cx);
    });
}

#[gpui::test]
async fn test_command_write_range(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file.rs"), "", cx);
    });

    cx.set_state(
        indoc! {"
                The quick
                brown« fox
                jumpsˇ» over
                the lazy dog
            "},
        Mode::Visual,
    );

    cx.simulate_keystrokes(": w space dir/other.rs");
    cx.simulate_keystrokes("enter");

    let other = path!("/root/dir/other.rs");

    let _ = cx
        .workspace(|workspace, window, cx| {
            workspace.open_abs_path(PathBuf::from(other), OpenOptions::default(), window, cx)
        })
        .await;

    cx.workspace(|workspace, _, cx| {
        assert_active_item(
            workspace,
            other,
            indoc! {"
                    brown fox
                    jumps over
                "},
            cx,
        );
    });
}

#[gpui::test]
async fn test_command_tabnew(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Create a new file to ensure that, when the filename is used with
    // `:tabnew`, it opens the existing file in a new tab.
    let fs = cx.workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());
    fs.as_fake()
        .insert_file(path!("/root/dir/file_2.rs"), "file_2".as_bytes().to_vec())
        .await;

    cx.simulate_keystrokes(": tabnew");
    cx.simulate_keystrokes("enter");
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 2));

    // Assert that the new tab is empty and not associated with any file, as
    // no file path was provided to the `:tabnew` command.
    cx.workspace(|workspace, _window, cx| {
        let active_editor = workspace.active_item_as::<Editor>(cx).unwrap();
        let buffer = active_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap();

        assert!(&buffer.read(cx).file().is_none());
    });

    // Leverage the filename as an argument to the `:tabnew` command,
    // ensuring that the file, instead of an empty buffer, is opened in a
    // new tab.
    cx.simulate_keystrokes(": tabnew space dir/file_2.rs");
    cx.simulate_keystrokes("enter");

    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 3));
    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file_2.rs"), "file_2", cx);
    });

    // If the `filename` argument provided to the `:tabnew` command is for a
    // file that doesn't yet exist, it should still associate the buffer
    // with that file path, so that when the buffer contents are saved, the
    // file is created.
    cx.simulate_keystrokes(": tabnew space dir/file_3.rs");
    cx.simulate_keystrokes("enter");

    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 4));
    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file_3.rs"), "", cx);
    });
}

#[gpui::test]
async fn test_command_tabedit(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // Create a new file to ensure that, when the filename is used with
    // `:tabedit`, it opens the existing file in a new tab.
    let fs = cx.workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());
    fs.as_fake()
        .insert_file(path!("/root/dir/file_2.rs"), "file_2".as_bytes().to_vec())
        .await;

    cx.simulate_keystrokes(": tabedit");
    cx.simulate_keystrokes("enter");
    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 2));

    // Assert that the new tab is empty and not associated with any file, as
    // no file path was provided to the `:tabedit` command.
    cx.workspace(|workspace, _window, cx| {
        let active_editor = workspace.active_item_as::<Editor>(cx).unwrap();
        let buffer = active_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap();

        assert!(&buffer.read(cx).file().is_none());
    });

    // Leverage the filename as an argument to the `:tabedit` command,
    // ensuring that the file, instead of an empty buffer, is opened in a
    // new tab.
    cx.simulate_keystrokes(": tabedit space dir/file_2.rs");
    cx.simulate_keystrokes("enter");

    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 3));
    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file_2.rs"), "file_2", cx);
    });

    // If the `filename` argument provided to the `:tabedit` command is for a
    // file that doesn't yet exist, it should still associate the buffer
    // with that file path, so that when the buffer contents are saved, the
    // file is created.
    cx.simulate_keystrokes(": tabedit space dir/file_3.rs");
    cx.simulate_keystrokes("enter");

    cx.workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 4));
    cx.workspace(|workspace, _, cx| {
        assert_active_item(workspace, path!("/root/dir/file_3.rs"), "", cx);
    });
}

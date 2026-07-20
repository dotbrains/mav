use super::*;

#[gpui::test]
async fn test_copy_paste_directory(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "a": {
                "one.txt": "",
                "two.txt": "",
                "inner_dir": {
                    "three.txt": "",
                    "four.txt": "",
                }
            },
            "b": {},
            "d.1.20": {
                "default.conf": "",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    select_path(&panel, "root/a", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
        panel.select_next(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    let pasted_dir = find_project_entry(&panel, "root/b/a", cx);
    assert_ne!(pasted_dir, None, "Pasted directory should have an entry");

    let pasted_dir_file = find_project_entry(&panel, "root/b/a/one.txt", cx);
    assert_ne!(
        pasted_dir_file, None,
        "Pasted directory file should have an entry"
    );

    let pasted_dir_inner_dir = find_project_entry(&panel, "root/b/a/inner_dir", cx);
    assert_ne!(
        pasted_dir_inner_dir, None,
        "Directories inside pasted directory should have an entry"
    );

    toggle_expand_dir(&panel, "root/b/a", cx);
    toggle_expand_dir(&panel, "root/b/a/inner_dir", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root",
            "    > a",
            "    v b",
            "        v a",
            "            v inner_dir  <== selected",
            "                  four.txt",
            "                  three.txt",
            "              one.txt",
            "              two.txt",
            "    > d.1.20",
        ]
    );

    select_path(&panel, "root", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root",
            "    > a",
            "    > [EDITOR: 'a copy']  <== selected",
            "    v b",
            "        v a",
            "            v inner_dir",
            "                  four.txt",
            "                  three.txt",
            "              one.txt",
            "              two.txt",
            "    > d.1.20",
        ]
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel
            .filename_editor
            .update(cx, |editor, cx| editor.set_text("c", window, cx));
        panel.confirm_edit(true, window, cx).unwrap()
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root",
            "    > a",
            "    > [PROCESSING: 'c']  <== selected",
            "    v b",
            "        v a",
            "            v inner_dir",
            "                  four.txt",
            "                  three.txt",
            "              one.txt",
            "              two.txt",
            "    > d.1.20",
        ]
    );

    confirm.await.unwrap();

    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root",
            "    > a",
            "    v b",
            "        v a",
            "            v inner_dir",
            "                  four.txt",
            "                  three.txt",
            "              one.txt",
            "              two.txt",
            "    v c",
            "        > a  <== selected",
            "        > inner_dir",
            "          one.txt",
            "          two.txt",
            "    > d.1.20",
        ]
    );

    select_path(&panel, "root/d.1.20", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root",
            "    > a",
            "    v b",
            "        v a",
            "            v inner_dir",
            "                  four.txt",
            "                  three.txt",
            "              one.txt",
            "              two.txt",
            "    v c",
            "        > a",
            "        > inner_dir",
            "          one.txt",
            "          two.txt",
            "    v d.1.20",
            "          default.conf",
            "    > [EDITOR: 'd.1.20 copy']  <== selected",
        ],
        "Dotted directory names should not be split at the dot when disambiguating"
    );
}

#[gpui::test]
async fn test_copy_paste_directory_with_sibling_file(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/test",
        json!({
            "dir1": {
                "a.txt": "",
                "b.txt": "",
            },
            "dir2": {},
            "c.txt": "",
            "d.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "test/dir1", cx);

    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });

    select_path_with_mark(&panel, "test/dir1", cx);
    select_path_with_mark(&panel, "test/c.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v test",
            "    v dir1  <== marked",
            "          a.txt",
            "          b.txt",
            "    > dir2",
            "      c.txt  <== selected  <== marked",
            "      d.txt",
        ],
        "Initial state before copying dir1 and c.txt"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });
    select_path(&panel, "test/dir2", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    toggle_expand_dir(&panel, "test/dir2/dir1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v test",
            "    v dir1  <== marked",
            "          a.txt",
            "          b.txt",
            "    v dir2",
            "        v dir1  <== selected",
            "              a.txt",
            "              b.txt",
            "          c.txt",
            "      c.txt  <== marked",
            "      d.txt",
        ],
        "Should copy dir1 as well as c.txt into dir2"
    );

    // Disambiguating multiple files should not open the rename editor.
    select_path(&panel, "test/dir2", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v test",
            "    v dir1  <== marked",
            "          a.txt",
            "          b.txt",
            "    v dir2",
            "        v dir1",
            "              a.txt",
            "              b.txt",
            "        > dir1 copy  <== selected",
            "          c.txt",
            "          c copy.txt",
            "      c.txt  <== marked",
            "      d.txt",
        ],
        "Should copy dir1 as well as c.txt into dir2 and disambiguate them without opening the rename editor"
    );
}

#[gpui::test]
async fn test_copy_paste_nested_and_root_entries(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/test",
        json!({
            "dir1": {
                "a.txt": "",
                "b.txt": "",
            },
            "dir2": {},
            "c.txt": "",
            "d.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/test".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "test/dir1", cx);

    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });

    select_path_with_mark(&panel, "test/dir1/a.txt", cx);
    select_path_with_mark(&panel, "test/dir1", cx);
    select_path_with_mark(&panel, "test/c.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v test",
            "    v dir1  <== marked",
            "          a.txt  <== marked",
            "          b.txt",
            "    > dir2",
            "      c.txt  <== selected  <== marked",
            "      d.txt",
        ],
        "Initial state before copying a.txt, dir1 and c.txt"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });
    select_path(&panel, "test/dir2", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    toggle_expand_dir(&panel, "test/dir2/dir1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v test",
            "    v dir1  <== marked",
            "          a.txt  <== marked",
            "          b.txt",
            "    v dir2",
            "        v dir1  <== selected",
            "              a.txt",
            "              b.txt",
            "          c.txt",
            "      c.txt  <== marked",
            "      d.txt",
        ],
        "Should copy dir1 and c.txt into dir2. a.txt is already present in copied dir1."
    );
}

#[gpui::test]
async fn test_paste_external_paths(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    set_auto_open_settings(
        cx,
        ProjectPanelAutoOpenSettings {
            on_drop: Some(false),
            ..Default::default()
        },
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "subdir": {}
        }),
    )
    .await;

    fs.insert_tree(
        path!("/external"),
        json!({
            "new_file.rs": "fn main() {}"
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    cx.write_to_clipboard(ClipboardItem {
        entries: vec![GpuiClipboardEntry::ExternalPaths(ExternalPaths(
            smallvec::smallvec![PathBuf::from(path!("/external/new_file.rs"))],
        ))],
    });

    select_path(&panel, "root/subdir", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "    v subdir",
            "          new_file.rs  <== selected",
        ],
    );
}

#[gpui::test]
async fn test_copy_and_cut_write_to_system_clipboard(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "file_a.txt": "",
            "file_b.txt": ""
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    select_path(&panel, "root/file_a.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });

    let clipboard = cx
        .read_from_clipboard()
        .expect("clipboard should have content after copy");
    let text = clipboard.text().expect("clipboard should contain text");
    assert!(
        text.contains("file_a.txt"),
        "System clipboard should contain the copied file path, got: {text}"
    );

    select_path(&panel, "root/file_b.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.cut(&Default::default(), window, cx);
    });

    let clipboard = cx
        .read_from_clipboard()
        .expect("clipboard should have content after cut");
    let text = clipboard.text().expect("clipboard should contain text");
    assert!(
        text.contains("file_b.txt"),
        "System clipboard should contain the cut file path, got: {text}"
    );
}

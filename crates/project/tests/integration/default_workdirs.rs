use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_block_via_channel(cx: &mut gpui::TestAppContext) {
    cx.executor().allow_parking();

    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let _thread = std::thread::spawn(move || {
        #[cfg(not(target_os = "windows"))]
        std::fs::metadata("/tmp").unwrap();
        #[cfg(target_os = "windows")]
        std::fs::metadata("C:/Windows").unwrap();
        std::thread::sleep(Duration::from_millis(1000));
        tx.unbounded_send(1).unwrap();
    });
    rx.next().await.unwrap();
}

#[gpui::test]
async fn test_block_via_smol(cx: &mut gpui::TestAppContext) {
    cx.executor().allow_parking();

    let io_task = smol::unblock(move || {
        println!("sleeping on thread {:?}", std::thread::current().id());
        std::thread::sleep(Duration::from_millis(10));
        1
    });

    let task = cx.foreground_executor().spawn(async move {
        io_task.await;
    });

    task.await;
}

#[gpui::test]
async fn test_default_session_work_dirs_prefers_directory_worktrees_over_single_file_parents(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir-project": {
                "src": {
                    "main.rs": "fn main() {}"
                }
            },
            "single-file.rs": "fn helper() {}"
        }),
    )
    .await;

    let project = Project::test(
        fs,
        [
            Path::new(path!("/root/single-file.rs")),
            Path::new(path!("/root/dir-project")),
        ],
        cx,
    )
    .await;

    let work_dirs = project.read_with(cx, |project, cx| project.default_path_list(cx));
    let ordered_paths = work_dirs.ordered_paths().cloned().collect::<Vec<_>>();

    assert_eq!(
        ordered_paths,
        vec![
            PathBuf::from(path!("/root/dir-project")),
            PathBuf::from(path!("/root")),
        ]
    );
}

#[gpui::test]
async fn test_default_session_work_dirs_falls_back_to_home_for_empty_project(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let work_dirs = project.read_with(cx, |project, cx| project.default_path_list(cx));
    let ordered_paths = work_dirs.ordered_paths().cloned().collect::<Vec<_>>();

    assert_eq!(ordered_paths, vec![paths::home_dir().to_path_buf()]);
}

// NOTE:
// While POSIX symbolic links are somewhat supported on Windows, they are an opt in by the user, and thus
// we assume that they are not supported out of the box.
#[cfg(not(windows))]
#[gpui::test]
async fn test_symlinks(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let dir = TempTree::new(json!({
        "root": {
            "apple": "",
            "banana": {
                "carrot": {
                    "date": "",
                    "endive": "",
                }
            },
            "fennel": {
                "grape": "",
            }
        }
    }));

    let root_link_path = dir.path().join("root_link");
    os::unix::fs::symlink(dir.path().join("root"), &root_link_path).unwrap();
    os::unix::fs::symlink(
        dir.path().join("root/fennel"),
        dir.path().join("root/finnochio"),
    )
    .unwrap();

    let project = Project::test(
        Arc::new(RealFs::new(None, cx.executor())),
        [root_link_path.as_ref()],
        cx,
    )
    .await;

    project.update(cx, |project, cx| {
        let tree = project.worktrees(cx).next().unwrap().read(cx);
        assert_eq!(tree.file_count(), 5);
        assert_eq!(
            tree.entry_for_path(rel_path("fennel/grape")).unwrap().inode,
            tree.entry_for_path(rel_path("finnochio/grape"))
                .unwrap()
                .inode
        );
    });
}

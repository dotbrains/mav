use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_create_entry(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/one/two",
        json!({
            "three": {
                "a.txt": "",
                "four": {}
            },
            "c.rs": ""
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/one/two/three".as_ref()], cx).await;
    project
        .update(cx, |project, cx| {
            let id = project.worktrees(cx).next().unwrap().read(cx).id();
            project.create_entry((id, rel_path("b..")), true, cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();

    assert_eq!(
        fs.paths(true),
        vec![
            PathBuf::from(path!("/")),
            PathBuf::from(path!("/one")),
            PathBuf::from(path!("/one/two")),
            PathBuf::from(path!("/one/two/c.rs")),
            PathBuf::from(path!("/one/two/three")),
            PathBuf::from(path!("/one/two/three/a.txt")),
            PathBuf::from(path!("/one/two/three/b..")),
            PathBuf::from(path!("/one/two/three/four")),
        ]
    );
}

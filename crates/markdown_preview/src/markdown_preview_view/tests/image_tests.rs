use super::*;

#[test]
fn resolves_workspace_absolute_preview_image_path_and_rejects_missing() {
    let tree = TempTree::new(json!({
        "docs": {},
        "test_image.png": "mock data"
    }));
    let workspace_directory = tree.path();
    let base_directory = markdown_fixture_directory(&tree);
    let image_file = workspace_directory.join("test_image.png");

    for workspace_root_relative_path in ["/test_image.png", "\\test_image.png"] {
        let resolved = resolve_preview_image(
            workspace_root_relative_path,
            Some(&base_directory),
            Some(workspace_directory),
        );
        assert_resolved_preview_image_path(resolved, image_file.as_path());
    }

    let missing = resolve_preview_image(
        "/missing_image.png",
        Some(&base_directory),
        Some(workspace_directory),
    );
    assert!(missing.is_none());
}

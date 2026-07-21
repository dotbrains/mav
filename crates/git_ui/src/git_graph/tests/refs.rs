use super::*;

#[test]
fn test_ref_name_from_decoration() {
    assert_eq!(
        GitGraph::ref_name_from_decoration("HEAD -> main"),
        Some("main".into())
    );
    assert_eq!(
        GitGraph::ref_name_from_decoration("main"),
        Some("main".into())
    );
    assert_eq!(
        GitGraph::ref_name_from_decoration("origin/main"),
        Some("origin/main".into())
    );
    assert_eq!(
        GitGraph::ref_name_from_decoration("tag: v1.0"),
        Some("v1.0".into())
    );
    assert_eq!(GitGraph::ref_name_from_decoration("HEAD"), None);
}

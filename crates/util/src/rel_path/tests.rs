use super::*;
use itertools::Itertools;
use pretty_assertions::assert_matches;

#[test]
fn test_rel_path_new() {
    assert!(RelPath::new(Path::new("/"), PathStyle::local()).is_err());
    assert!(RelPath::new(Path::new("//"), PathStyle::local()).is_err());
    assert!(RelPath::new(Path::new("/foo/"), PathStyle::local()).is_err());

    let path = RelPath::new("foo/".as_ref(), PathStyle::local()).unwrap();
    assert_eq!(path, rel_path("foo").into());
    assert_matches!(path, Cow::Borrowed(_));

    let path = RelPath::new("foo\\".as_ref(), PathStyle::Windows).unwrap();
    assert_eq!(path, rel_path("foo").into());
    assert_matches!(path, Cow::Borrowed(_));

    assert_eq!(
        RelPath::new("foo/bar/../baz/./quux/".as_ref(), PathStyle::local())
            .unwrap()
            .as_ref(),
        rel_path("foo/baz/quux")
    );

    let path = RelPath::new("./foo/bar".as_ref(), PathStyle::Posix).unwrap();
    assert_eq!(path.as_ref(), rel_path("foo/bar"));
    assert_matches!(path, Cow::Borrowed(_));

    let path = RelPath::new(".\\foo".as_ref(), PathStyle::Windows).unwrap();
    assert_eq!(path, rel_path("foo").into());
    assert_matches!(path, Cow::Borrowed(_));

    let path = RelPath::new("./.\\./foo/\\/".as_ref(), PathStyle::Windows).unwrap();
    assert_eq!(path, rel_path("foo").into());
    assert_matches!(path, Cow::Borrowed(_));

    let path = RelPath::new("foo/./bar".as_ref(), PathStyle::Posix).unwrap();
    assert_eq!(path.as_ref(), rel_path("foo/bar"));
    assert_matches!(path, Cow::Owned(_));

    let path = RelPath::new("./foo/bar".as_ref(), PathStyle::Windows).unwrap();
    assert_eq!(path.as_ref(), rel_path("foo/bar"));
    assert_matches!(path, Cow::Borrowed(_));

    let path = RelPath::new(".\\foo\\bar".as_ref(), PathStyle::Windows).unwrap();
    assert_eq!(path.as_ref(), rel_path("foo/bar"));
    assert_matches!(path, Cow::Owned(_));
}

#[test]
fn test_rel_path_components() {
    let path = rel_path("foo/bar/baz");
    assert_eq!(
        path.components().collect::<Vec<_>>(),
        vec!["foo", "bar", "baz"]
    );
    assert_eq!(
        path.components().rev().collect::<Vec<_>>(),
        vec!["baz", "bar", "foo"]
    );

    let path = rel_path("");
    let mut components = path.components();
    assert_eq!(components.next(), None);
}

#[test]
fn test_rel_path_ancestors() {
    let path = rel_path("foo/bar/baz");
    let mut ancestors = path.ancestors();
    assert_eq!(ancestors.next(), Some(rel_path("foo/bar/baz")));
    assert_eq!(ancestors.next(), Some(rel_path("foo/bar")));
    assert_eq!(ancestors.next(), Some(rel_path("foo")));
    assert_eq!(ancestors.next(), Some(rel_path("")));
    assert_eq!(ancestors.next(), None);

    let path = rel_path("foo");
    let mut ancestors = path.ancestors();
    assert_eq!(ancestors.next(), Some(rel_path("foo")));
    assert_eq!(ancestors.next(), Some(RelPath::empty()));
    assert_eq!(ancestors.next(), None);

    let path = RelPath::empty();
    let mut ancestors = path.ancestors();
    assert_eq!(ancestors.next(), Some(RelPath::empty()));
    assert_eq!(ancestors.next(), None);
}

#[test]
fn test_rel_path_parent() {
    assert_eq!(rel_path("foo/bar/baz").parent(), Some(rel_path("foo/bar")));
    assert_eq!(rel_path("foo").parent(), Some(RelPath::empty()));
    assert_eq!(rel_path("").parent(), None);
}

#[test]
fn test_rel_path_partial_ord_is_compatible_with_std() {
    let test_cases = ["a/b/c", "relative/path/with/dot.", "relative/path/with.dot"];
    for [lhs, rhs] in test_cases.iter().array_combinations::<2>() {
        assert_eq!(
            Path::new(lhs).cmp(Path::new(rhs)),
            RelPath::unix(lhs)
                .unwrap()
                .cmp(&RelPath::unix(rhs).unwrap())
        );
    }
}

#[test]
fn test_strip_prefix() {
    let parent = rel_path("");
    let child = rel_path(".foo");

    assert!(child.starts_with(parent));
    assert_eq!(child.strip_prefix(parent).unwrap(), child);
}

#[test]
fn test_rel_path_constructors_absolute_path() {
    assert!(RelPath::new(Path::new("/a/b"), PathStyle::Windows).is_err());
    assert!(RelPath::new(Path::new("\\a\\b"), PathStyle::Windows).is_err());
    assert!(RelPath::new(Path::new("/a/b"), PathStyle::Posix).is_err());
    assert!(RelPath::new(Path::new("C:/a/b"), PathStyle::Windows).is_err());
    assert!(RelPath::new(Path::new("C:\\a\\b"), PathStyle::Windows).is_err());
    assert!(RelPath::new(Path::new("C:/a/b"), PathStyle::Posix).is_ok());
}

#[test]
fn test_pop() {
    let mut path = rel_path("a/b").to_rel_path_buf();
    path.pop();
    assert_eq!(path.as_rel_path().as_unix_str(), "a");
    path.pop();
    assert_eq!(path.as_rel_path().as_unix_str(), "");
    path.pop();
    assert_eq!(path.as_rel_path().as_unix_str(), "");
}

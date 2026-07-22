use super::*;

#[cfg(target_os = "windows")]
#[test]
fn test_wsl_path() {
    use super::WslPath;
    let path = "/a/b/c";
    assert_eq!(WslPath::from_path(&path), None);

    let path = r"\\wsl.localhost";
    assert_eq!(WslPath::from_path(&path), None);

    let path = r"\\wsl.localhost\Distro";
    assert_eq!(
        WslPath::from_path(&path),
        Some(WslPath {
            distro: "Distro".to_owned(),
            path: "/".into(),
        })
    );

    let path = r"\\wsl.localhost\Distro\blue";
    assert_eq!(
        WslPath::from_path(&path),
        Some(WslPath {
            distro: "Distro".to_owned(),
            path: "/blue".into()
        })
    );

    let path = r"\\wsl$\archlinux\tomato\.\paprika\..\aubergine.txt";
    assert_eq!(
        WslPath::from_path(&path),
        Some(WslPath {
            distro: "archlinux".to_owned(),
            path: "/tomato/paprika/../aubergine.txt".into()
        })
    );

    let path = r"\\windows.localhost\Distro\foo";
    assert_eq!(WslPath::from_path(&path), None);
}

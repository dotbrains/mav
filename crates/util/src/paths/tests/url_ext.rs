use super::*;

#[test]
fn test_url_to_file_path_ext_posix_basic() {
    use super::UrlExt;

    let url = url::Url::parse("file:///home/user/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/home/user/file.txt"))
    );

    let url = url::Url::parse("file:///").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/"))
    );

    let url = url::Url::parse("file:///a/b/c/d/e").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/a/b/c/d/e"))
    );
}

#[test]
fn test_url_to_file_path_ext_posix_percent_encoding() {
    use super::UrlExt;

    let url = url::Url::parse("file:///home/user/file%20with%20spaces.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/home/user/file with spaces.txt"))
    );

    let url = url::Url::parse("file:///path%2Fwith%2Fencoded%2Fslashes").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/path/with/encoded/slashes"))
    );

    let url = url::Url::parse("file:///special%23chars%3F.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/special#chars?.txt"))
    );
}

#[test]
fn test_url_to_file_path_ext_posix_localhost() {
    use super::UrlExt;

    let url = url::Url::parse("file://localhost/home/user/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/home/user/file.txt"))
    );
}

#[test]
fn test_url_to_file_path_ext_posix_rejects_host() {
    use super::UrlExt;

    let url = url::Url::parse("file://somehost/home/user/file.txt").unwrap();
    assert_eq!(url.to_file_path_ext(PathStyle::Posix), Err(()));
}

#[test]
fn test_url_to_file_path_ext_posix_windows_drive_letter() {
    use super::UrlExt;

    let url = url::Url::parse("file:///C:").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/C:/"))
    );

    let url = url::Url::parse("file:///D|").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Posix),
        Ok(PathBuf::from("/D|/"))
    );
}

#[test]
fn test_url_to_file_path_ext_windows_basic() {
    use super::UrlExt;

    let url = url::Url::parse("file:///C:/Users/user/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("C:\\Users\\user\\file.txt"))
    );

    let url = url::Url::parse("file:///D:/folder/subfolder/file.rs").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("D:\\folder\\subfolder\\file.rs"))
    );

    let url = url::Url::parse("file:///C:/").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("C:\\"))
    );
}

#[test]
fn test_url_to_file_path_ext_windows_encoded_drive_letter() {
    use super::UrlExt;

    let url = url::Url::parse("file:///C%3A/Users/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("C:\\Users\\file.txt"))
    );

    let url = url::Url::parse("file:///c%3a/Users/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("c:\\Users\\file.txt"))
    );

    let url = url::Url::parse("file:///D%3A/folder/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("D:\\folder\\file.txt"))
    );

    let url = url::Url::parse("file:///d%3A/folder/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("d:\\folder\\file.txt"))
    );
}

#[test]
fn test_url_to_file_path_ext_windows_unc_path() {
    use super::UrlExt;

    let url = url::Url::parse("file://server/share/path/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("\\\\server\\share\\path\\file.txt"))
    );

    let url = url::Url::parse("file://server/share").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("\\\\server\\share"))
    );
}

#[test]
fn test_url_to_file_path_ext_windows_percent_encoding() {
    use super::UrlExt;

    let url = url::Url::parse("file:///C:/Users/user/file%20with%20spaces.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("C:\\Users\\user\\file with spaces.txt"))
    );

    let url = url::Url::parse("file:///C:/special%23chars%3F.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("C:\\special#chars?.txt"))
    );
}

#[test]
fn test_url_to_file_path_ext_windows_invalid_drive() {
    use super::UrlExt;

    let url = url::Url::parse("file:///1:/path/file.txt").unwrap();
    assert_eq!(url.to_file_path_ext(PathStyle::Windows), Err(()));

    let url = url::Url::parse("file:///CC:/path/file.txt").unwrap();
    assert_eq!(url.to_file_path_ext(PathStyle::Windows), Err(()));

    let url = url::Url::parse("file:///C/path/file.txt").unwrap();
    assert_eq!(url.to_file_path_ext(PathStyle::Windows), Err(()));

    let url = url::Url::parse("file:///invalid").unwrap();
    assert_eq!(url.to_file_path_ext(PathStyle::Windows), Err(()));
}

#[test]
fn test_url_to_file_path_ext_non_file_scheme() {
    use super::UrlExt;

    let url = url::Url::parse("http://example.com/path").unwrap();
    assert_eq!(url.to_file_path_ext(PathStyle::Posix), Err(()));
    assert_eq!(url.to_file_path_ext(PathStyle::Windows), Err(()));

    let url = url::Url::parse("https://example.com/path").unwrap();
    assert_eq!(url.to_file_path_ext(PathStyle::Posix), Err(()));
    assert_eq!(url.to_file_path_ext(PathStyle::Windows), Err(()));
}

#[test]
fn test_url_to_file_path_ext_windows_localhost() {
    use super::UrlExt;

    let url = url::Url::parse("file://localhost/C:/Users/file.txt").unwrap();
    assert_eq!(
        url.to_file_path_ext(PathStyle::Windows),
        Ok(PathBuf::from("C:\\Users\\file.txt"))
    );
}

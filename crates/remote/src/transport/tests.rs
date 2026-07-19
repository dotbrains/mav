use super::*;

#[test]
fn test_parse_platform() {
    let result = parse_platform("Linux x86_64\n").unwrap();
    assert_eq!(result.os, RemoteOs::Linux);
    assert_eq!(result.arch, RemoteArch::X86_64);

    let result = parse_platform("Darwin arm64\n").unwrap();
    assert_eq!(result.os, RemoteOs::MacOs);
    assert_eq!(result.arch, RemoteArch::Aarch64);

    let result = parse_platform("Linux x86_64").unwrap();
    assert_eq!(result.os, RemoteOs::Linux);
    assert_eq!(result.arch, RemoteArch::X86_64);

    let result = parse_platform("some shell init output\nLinux aarch64\n").unwrap();
    assert_eq!(result.os, RemoteOs::Linux);
    assert_eq!(result.arch, RemoteArch::Aarch64);

    let result = parse_platform("some shell init output\nLinux aarch64").unwrap();
    assert_eq!(result.os, RemoteOs::Linux);
    assert_eq!(result.arch, RemoteArch::Aarch64);

    assert_eq!(
        parse_platform("Linux armv8l\n").unwrap().arch,
        RemoteArch::Aarch64
    );
    assert_eq!(
        parse_platform("Linux aarch64\n").unwrap().arch,
        RemoteArch::Aarch64
    );
    assert_eq!(
        parse_platform("Linux x86_64\n").unwrap().arch,
        RemoteArch::X86_64
    );

    let result = parse_platform(
        r#"Linux x86_64 - What you're referring to as Linux, is in fact, GNU/Linux...\n"#,
    )
    .unwrap();
    assert_eq!(result.os, RemoteOs::Linux);
    assert_eq!(result.arch, RemoteArch::X86_64);

    assert!(parse_platform("Windows x86_64\n").is_err());
    assert!(parse_platform("Linux armv7l\n").is_err());
}

#[test]
fn test_parse_os_version() {
    let os_release = "ID=ubuntu\nVERSION_ID=\"24.04\"\n";
    assert_eq!(
        parse_os_version(RemoteOs::Linux, os_release),
        Some("ubuntu 24.04".to_string())
    );

    assert_eq!(
        parse_os_version(RemoteOs::MacOs, "15.6.1\n"),
        Some("15.6.1".to_string())
    );
    assert_eq!(
        parse_os_version(RemoteOs::MacOs, "shell noise\n26.0\n"),
        Some("26.0".to_string())
    );
    assert_eq!(parse_os_version(RemoteOs::MacOs, ""), None);

    assert_eq!(
        parse_os_version(
            RemoteOs::Windows,
            "Microsoft Windows [Version 10.0.19045.5011]\n"
        ),
        Some("10.0.19045".to_string())
    );
    assert_eq!(
        parse_os_version(
            RemoteOs::Windows,
            "Microsoft Windows [Versione 10.0.22631.1]"
        ),
        Some("10.0.22631".to_string())
    );
    assert_eq!(parse_os_version(RemoteOs::Windows, "no version here"), None);
}

#[test]
fn test_parse_shell() {
    assert_eq!(parse_shell("/bin/bash\n", "sh"), "/bin/bash");
    assert_eq!(parse_shell("/bin/zsh\n", "sh"), "/bin/zsh");

    assert_eq!(parse_shell("/bin/bash", "sh"), "/bin/bash");
    assert_eq!(
        parse_shell("some shell init output\n/bin/bash\n", "sh"),
        "/bin/bash"
    );
    assert_eq!(
        parse_shell("some shell init output\n/bin/bash", "sh"),
        "/bin/bash"
    );
    assert_eq!(parse_shell("", "sh"), "sh");
    assert_eq!(parse_shell("\n", "sh"), "sh");
}

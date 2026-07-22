use super::*;

#[test]
fn test_package_name_from_pkgid() {
    for (input, expected) in [
        (
            "path+file:///absolute/path/to/project/mav/crates/mav#0.131.0",
            "mav",
        ),
        (
            "path+file:///absolute/path/to/project/custom-package#my-custom-package@0.1.0",
            "my-custom-package",
        ),
    ] {
        assert_eq!(package_name_from_pkgid(input), Some(expected));
    }
}

#[test]
fn test_target_info_from_metadata() {
    for (input, absolute_path, expected) in [
        (
            r#"{"packages":[{"id":"path+file:///absolute/path/to/project/mav/crates/mav#0.131.0","manifest_path":"/path/to/mav/Cargo.toml","targets":[{"name":"mav","kind":["bin"],"src_path":"/path/to/mav/src/main.rs"}]}]}"#,
            "/path/to/mav/src/main.rs",
            Some((
                Some(TargetInfo {
                    package_name: "mav".into(),
                    target_name: "mav".into(),
                    required_features: Vec::new(),
                    target_kind: TargetKind::Bin,
                }),
                Arc::from("/path/to/mav".as_ref()),
            )),
        ),
        (
            r#"{"packages":[{"id":"path+file:///path/to/custom-package#my-custom-package@0.1.0","manifest_path":"/path/to/custom-package/Cargo.toml","targets":[{"name":"my-custom-bin","kind":["bin"],"src_path":"/path/to/custom-package/src/main.rs"}]}]}"#,
            "/path/to/custom-package/src/main.rs",
            Some((
                Some(TargetInfo {
                    package_name: "my-custom-package".into(),
                    target_name: "my-custom-bin".into(),
                    required_features: Vec::new(),
                    target_kind: TargetKind::Bin,
                }),
                Arc::from("/path/to/custom-package".as_ref()),
            )),
        ),
        (
            r#"{"packages":[{"id":"path+file:///path/to/custom-package#my-custom-package@0.1.0","targets":[{"name":"my-custom-bin","kind":["example"],"src_path":"/path/to/custom-package/src/main.rs"}],"manifest_path":"/path/to/custom-package/Cargo.toml"}]}"#,
            "/path/to/custom-package/src/main.rs",
            Some((
                Some(TargetInfo {
                    package_name: "my-custom-package".into(),
                    target_name: "my-custom-bin".into(),
                    required_features: Vec::new(),
                    target_kind: TargetKind::Example,
                }),
                Arc::from("/path/to/custom-package".as_ref()),
            )),
        ),
        (
            r#"{"packages":[{"id":"path+file:///path/to/custom-package#my-custom-package@0.1.0","manifest_path":"/path/to/custom-package/Cargo.toml","targets":[{"name":"my-custom-bin","kind":["example"],"src_path":"/path/to/custom-package/src/main.rs","required-features":["foo","bar"]}]}]}"#,
            "/path/to/custom-package/src/main.rs",
            Some((
                Some(TargetInfo {
                    package_name: "my-custom-package".into(),
                    target_name: "my-custom-bin".into(),
                    required_features: vec!["foo".to_owned(), "bar".to_owned()],
                    target_kind: TargetKind::Example,
                }),
                Arc::from("/path/to/custom-package".as_ref()),
            )),
        ),
        (
            r#"{"packages":[{"id":"path+file:///path/to/custom-package#my-custom-package@0.1.0","targets":[{"name":"my-custom-bin","kind":["example"],"src_path":"/path/to/custom-package/src/main.rs","required-features":[]}],"manifest_path":"/path/to/custom-package/Cargo.toml"}]}"#,
            "/path/to/custom-package/src/main.rs",
            Some((
                Some(TargetInfo {
                    package_name: "my-custom-package".into(),
                    target_name: "my-custom-bin".into(),
                    required_features: vec![],
                    target_kind: TargetKind::Example,
                }),
                Arc::from("/path/to/custom-package".as_ref()),
            )),
        ),
        (
            r#"{"packages":[{"id":"path+file:///path/to/custom-package#my-custom-package@0.1.0","targets":[{"name":"my-custom-package","kind":["lib"],"src_path":"/path/to/custom-package/src/main.rs"}],"manifest_path":"/path/to/custom-package/Cargo.toml"}]}"#,
            "/path/to/custom-package/src/main.rs",
            Some((None, Arc::from("/path/to/custom-package".as_ref()))),
        ),
    ] {
        let metadata: CargoMetadata = serde_json::from_str(input).context(input).unwrap();

        let absolute_path = Path::new(absolute_path);

        assert_eq!(target_info_from_metadata(metadata, absolute_path), expected);
    }
}

#[test]
fn target_info_from_abs_path_failed() {
    let project_root = tempfile::tempdir().unwrap();
    let cargo_toml_path = project_root.path().join("Cargo.toml");
    let src_dir = project_root.path().join("src");
    let main_rs_path = src_dir.join("main.rs");

    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(&cargo_toml_path, "invalid_toml = {[[{").unwrap();
    std::fs::write(&main_rs_path, "// rust").unwrap();

    let e = smol::block_on(target_info_from_abs_path(&main_rs_path, None)).unwrap_err();
    assert!(e.to_string().contains("Cargo metadata failed"));
}

#[test]
fn test_rust_test_fragment() {
    #[track_caller]
    fn check(
        variables: impl IntoIterator<Item = (VariableName, &'static str)>,
        path: &str,
        expected: &str,
    ) {
        let path = Path::new(path);
        let found = test_fragment(
            &TaskVariables::from_iter(variables.into_iter().map(|(k, v)| (k, v.to_owned()))),
            path,
            path.file_stem().unwrap().to_str().unwrap(),
        );
        assert_eq!(expected, found);
    }

    check([], "/project/src/lib.rs", "--lib");
    check([], "/project/src/foo/mod.rs", "foo");
    check(
        [
            (RUST_BIN_KIND_TASK_VARIABLE.clone(), "bin"),
            (RUST_BIN_NAME_TASK_VARIABLE, "x"),
        ],
        "/project/src/main.rs",
        "--bin=x",
    );
    check([], "/project/src/main.rs", "--");
}

#[test]
fn test_convert_rust_analyzer_schema() {
    let raw_schema = serde_json::json!([
        {
            "title": "Assist",
            "properties": {
                "rust-analyzer.assist.emitMustUse": {
                    "markdownDescription": "Insert #[must_use] when generating `as_` methods for enum variants.",
                    "default": false,
                    "type": "boolean"
                }
            }
        },
        {
            "title": "Assist",
            "properties": {
                "rust-analyzer.assist.expressionFillDefault": {
                    "markdownDescription": "Placeholder expression to use for missing expressions in assists.",
                    "default": "todo",
                    "type": "string"
                }
            }
        },
        {
            "title": "Cache Priming",
            "properties": {
                "rust-analyzer.cachePriming.enable": {
                    "markdownDescription": "Warm up caches on project load.",
                    "default": true,
                    "type": "boolean"
                }
            }
        }
    ]);

    let converted = RustLspAdapter::convert_rust_analyzer_schema(&raw_schema);

    assert_eq!(
        converted.get("type").and_then(|v| v.as_str()),
        Some("object")
    );

    let properties = converted
        .pointer("/properties")
        .expect("should have properties")
        .as_object()
        .expect("properties should be object");

    assert!(properties.contains_key("assist"));
    assert!(properties.contains_key("cachePriming"));
    assert!(!properties.contains_key("rust-analyzer"));

    let assist_props = properties
        .get("assist")
        .expect("should have assist")
        .pointer("/properties")
        .expect("assist should have properties")
        .as_object()
        .expect("assist properties should be object");

    assert!(assist_props.contains_key("emitMustUse"));
    assert!(assist_props.contains_key("expressionFillDefault"));

    let emit_must_use = assist_props
        .get("emitMustUse")
        .expect("should have emitMustUse");
    assert_eq!(
        emit_must_use.get("type").and_then(|v| v.as_str()),
        Some("boolean")
    );
    assert_eq!(
        emit_must_use.get("default").and_then(|v| v.as_bool()),
        Some(false)
    );

    let cache_priming_props = properties
        .get("cachePriming")
        .expect("should have cachePriming")
        .pointer("/properties")
        .expect("cachePriming should have properties")
        .as_object()
        .expect("cachePriming properties should be object");

    assert!(cache_priming_props.contains_key("enable"));
}

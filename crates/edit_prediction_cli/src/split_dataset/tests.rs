use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_temp_jsonl(lines: &[&str]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    for line in lines {
        writeln!(file, "{}", line).unwrap();
    }
    file.flush().unwrap();
    file
}

#[test]
fn test_parse_split_spec_percentage() {
    let spec = parse_split_spec("train.jsonl=80%").unwrap();
    assert_eq!(spec.path, PathBuf::from("train.jsonl"));
    match spec.size {
        SplitSize::Percentage(p) => assert!((p - 0.8).abs() < 0.001),
        _ => panic!("expected percentage"),
    }
}

#[test]
fn test_parse_split_spec_absolute() {
    let spec = parse_split_spec("test.jsonl=100").unwrap();
    assert_eq!(spec.path, PathBuf::from("test.jsonl"));
    match spec.size {
        SplitSize::Absolute(n) => assert_eq!(n, 100),
        _ => panic!("expected absolute"),
    }
}

#[test]
fn test_parse_split_spec_rest() {
    let spec = parse_split_spec("valid.jsonl=rest").unwrap();
    assert_eq!(spec.path, PathBuf::from("valid.jsonl"));
    assert!(matches!(spec.size, SplitSize::Rest));
}

#[test]
fn test_group_lines_none() {
    let lines = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let groups = group_lines(&lines, Stratify::None);
    assert_eq!(groups.len(), 3);
    assert!(groups.iter().all(|g| g.len() == 1));
}

#[test]
fn test_compute_split_counts_percentage() {
    let specs = vec![
        SplitSpec {
            path: PathBuf::from("a"),
            size: SplitSize::Percentage(0.8),
        },
        SplitSpec {
            path: PathBuf::from("b"),
            size: SplitSize::Percentage(0.2),
        },
    ];
    let counts = compute_split_counts(&specs, 100).unwrap();
    assert_eq!(counts, vec![80, 20]);
}

#[test]
fn test_compute_split_counts_with_rest() {
    let specs = vec![
        SplitSpec {
            path: PathBuf::from("a"),
            size: SplitSize::Percentage(0.8),
        },
        SplitSpec {
            path: PathBuf::from("b"),
            size: SplitSize::Rest,
        },
    ];
    let counts = compute_split_counts(&specs, 100).unwrap();
    assert_eq!(counts, vec![80, 20]);
}

#[test]
fn test_compute_split_counts_absolute() {
    let specs = vec![
        SplitSpec {
            path: PathBuf::from("a"),
            size: SplitSize::Absolute(50),
        },
        SplitSpec {
            path: PathBuf::from("b"),
            size: SplitSize::Rest,
        },
    ];
    let counts = compute_split_counts(&specs, 100).unwrap();
    assert_eq!(counts, vec![50, 50]);
}

#[test]
fn test_group_lines_by_repo() {
    let lines = vec![
        r#"{"repository_url": "repo1", "id": 1}"#.to_string(),
        r#"{"repository_url": "repo1", "id": 2}"#.to_string(),
        r#"{"repository_url": "repo2", "id": 3}"#.to_string(),
        r#"{"id": 4}"#.to_string(),
    ];

    let groups = group_lines(&lines, Stratify::Repo);

    let grouped_count: usize = groups.iter().filter(|g| g.len() > 1).count();
    let ungrouped_count: usize = groups.iter().filter(|g| g.len() == 1).count();
    let total_lines: usize = groups.iter().map(|g| g.len()).sum();

    assert_eq!(grouped_count, 1);
    assert_eq!(ungrouped_count, 2);
    assert_eq!(total_lines, 4);
}

#[test]
fn test_group_lines_by_cursor_path() {
    let lines = vec![
        r#"{"cursor_path": "src/main.rs", "id": 1}"#.to_string(),
        r#"{"cursor_path": "src/main.rs", "id": 2}"#.to_string(),
        r#"{"cursor_path": "src/lib.rs", "id": 3}"#.to_string(),
    ];

    let groups = group_lines(&lines, Stratify::CursorPath);

    let total_lines: usize = groups.iter().map(|g| g.len()).sum();
    assert_eq!(groups.len(), 2);
    assert_eq!(total_lines, 3);
}

#[test]
fn test_run_split_basic() {
    let input = create_temp_jsonl(&[
        r#"{"repository_url": "repo1", "id": 1}"#,
        r#"{"repository_url": "repo1", "id": 2}"#,
        r#"{"repository_url": "repo2", "id": 3}"#,
        r#"{"repository_url": "repo2", "id": 4}"#,
        r#"{"repository_url": "repo3", "id": 5}"#,
        r#"{"repository_url": "repo3", "id": 6}"#,
        r#"{"repository_url": "repo4", "id": 7}"#,
        r#"{"repository_url": "repo4", "id": 8}"#,
    ]);

    let temp_dir = tempfile::tempdir().unwrap();
    let train_path = temp_dir.path().join("train.jsonl");
    let valid_path = temp_dir.path().join("valid.jsonl");

    let args = SplitArgs {
        seed: Some(42),
        stratify: Stratify::Repo,
    };
    let inputs = vec![
        input.path().to_path_buf(),
        PathBuf::from(format!("{}=50%", train_path.display())),
        PathBuf::from(format!("{}=rest", valid_path.display())),
    ];

    run_split(&args, &inputs).unwrap();

    let train_content = std::fs::read_to_string(&train_path).unwrap();
    let valid_content = std::fs::read_to_string(&valid_path).unwrap();

    let train_lines: Vec<&str> = train_content.lines().collect();
    let valid_lines: Vec<&str> = valid_content.lines().collect();

    assert_eq!(train_lines.len() + valid_lines.len(), 8);

    let get_repo = |line: &str| -> Option<String> {
        let value: Value = serde_json::from_str(line).ok()?;
        value
            .get("repository_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };

    let train_repos: std::collections::HashSet<_> =
        train_lines.iter().filter_map(|l| get_repo(l)).collect();
    let valid_repos: std::collections::HashSet<_> =
        valid_lines.iter().filter_map(|l| get_repo(l)).collect();

    assert!(
        train_repos.is_disjoint(&valid_repos),
        "train and valid should have non-overlapping repos"
    );
}

#[test]
fn test_multiple_rest_fails() {
    let specs = vec![
        SplitSpec {
            path: PathBuf::from("a"),
            size: SplitSize::Rest,
        },
        SplitSpec {
            path: PathBuf::from("b"),
            size: SplitSize::Rest,
        },
    ];
    assert!(compute_split_counts(&specs, 100).is_err());
}

#[test]
fn test_absolute_targets_lines_not_groups() {
    let input = create_temp_jsonl(&[
        r#"{"repository_url": "r1", "id": 1}"#,
        r#"{"repository_url": "r1", "id": 2}"#,
        r#"{"repository_url": "r1", "id": 3}"#,
        r#"{"repository_url": "r2", "id": 4}"#,
        r#"{"repository_url": "r2", "id": 5}"#,
        r#"{"repository_url": "r2", "id": 6}"#,
        r#"{"repository_url": "r3", "id": 7}"#,
        r#"{"repository_url": "r3", "id": 8}"#,
        r#"{"repository_url": "r3", "id": 9}"#,
        r#"{"repository_url": "r4", "id": 10}"#,
        r#"{"repository_url": "r4", "id": 11}"#,
        r#"{"repository_url": "r4", "id": 12}"#,
        r#"{"repository_url": "r5", "id": 13}"#,
        r#"{"repository_url": "r5", "id": 14}"#,
        r#"{"repository_url": "r5", "id": 15}"#,
    ]);

    let temp_dir = tempfile::tempdir().unwrap();
    let train_path = temp_dir.path().join("train.jsonl");
    let valid_path = temp_dir.path().join("valid.jsonl");

    let args = SplitArgs {
        seed: Some(42),
        stratify: Stratify::Repo,
    };
    let inputs = vec![
        input.path().to_path_buf(),
        PathBuf::from(format!("{}=6", train_path.display())),
        PathBuf::from(format!("{}=rest", valid_path.display())),
    ];

    run_split(&args, &inputs).unwrap();

    let train_content = std::fs::read_to_string(&train_path).unwrap();
    let valid_content = std::fs::read_to_string(&valid_path).unwrap();

    let train_lines: Vec<&str> = train_content.lines().collect();
    let valid_lines: Vec<&str> = valid_content.lines().collect();

    assert_eq!(train_lines.len(), 6);
    assert_eq!(valid_lines.len(), 9);
}

#[test]
fn test_stratify_by_project() {
    let input = create_temp_jsonl(&[
        r#"{"cursor_path": "project1/some/file.rs", "id": 1}"#,
        r#"{"cursor_path": "project2/some/file.rs", "id": 2}"#,
        r#"{"cursor_path": "project3/some/file.rs", "id": 3}"#,
        r#"{"cursor_path": "project1/other/file.rs", "id": 4}"#,
        r#"{"cursor_path": "project2/other/file.rs", "id": 5}"#,
        r#"{"cursor_path": "project3/other/file.rs", "id": 6}"#,
        r#"{"cursor_path": "project3/another/file.rs", "id": 7}"#,
        r#"{"cursor_path": "project3/even/more.rs", "id": 8}"#,
    ]);

    let temp_dir = tempfile::tempdir().unwrap();
    let train_path = temp_dir.path().join("train.jsonl");
    let valid_path = temp_dir.path().join("valid.jsonl");

    let args = SplitArgs {
        seed: Some(1),
        stratify: Stratify::Project,
    };
    let inputs = vec![
        input.path().to_path_buf(),
        PathBuf::from(format!("{}=4", train_path.display())),
        PathBuf::from(format!("{}=rest", valid_path.display())),
    ];

    run_split(&args, &inputs).unwrap();

    let train_content = std::fs::read_to_string(&train_path).unwrap();
    let valid_content = std::fs::read_to_string(&valid_path).unwrap();

    let mut train_ids: Vec<u64> = train_content
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["id"]
                .as_u64()
                .unwrap()
        })
        .collect();
    let mut valid_ids: Vec<u64> = valid_content
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["id"]
                .as_u64()
                .unwrap()
        })
        .collect();

    train_ids.sort();
    valid_ids.sort();

    assert_eq!(train_ids, vec![1, 2, 4, 5]);
    assert_eq!(valid_ids, vec![3, 6, 7, 8]);
}

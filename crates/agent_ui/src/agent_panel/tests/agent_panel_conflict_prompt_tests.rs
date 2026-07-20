use super::*;

#[test]
fn test_build_conflict_resolution_prompt_single_conflict() {
    let conflicts = vec![ConflictContent {
        file_path: "src/main.rs".to_string(),
        conflict_text: "<<<<<<< HEAD\nlet x = 1;\n=======\nlet x = 2;\n>>>>>>> feature".to_string(),
        ours_branch_name: "HEAD".to_string(),
        theirs_branch_name: "feature".to_string(),
    }];

    let blocks = build_conflict_resolution_prompt(&conflicts);
    assert_eq!(
        blocks.len(),
        4,
        "expected 2 text + 1 resource link + 1 resource block"
    );

    let intro_text = expect_text_block(&blocks[0]);
    assert!(
        intro_text.contains("Please resolve the following merge conflict in"),
        "prompt should include single-conflict intro text"
    );

    match &blocks[1] {
        acp::ContentBlock::ResourceLink(link) => {
            assert!(
                link.uri.contains("file://"),
                "resource link URI should use file scheme"
            );
            assert!(
                link.uri.contains("main.rs"),
                "resource link URI should reference file path"
            );
        }
        other => panic!("expected ResourceLink block, got {:?}", other),
    }

    let body_text = expect_text_block(&blocks[2]);
    assert!(
        body_text.contains("`HEAD` (ours)"),
        "prompt should mention ours branch"
    );
    assert!(
        body_text.contains("`feature` (theirs)"),
        "prompt should mention theirs branch"
    );
    assert!(
        body_text.contains("editing the file directly"),
        "prompt should instruct the agent to edit the file"
    );

    let (resource_text, resource_uri) = expect_resource_block(&blocks[3]);
    assert!(
        resource_text.contains("<<<<<<< HEAD"),
        "resource should contain the conflict text"
    );
    assert!(
        resource_uri.contains("merge-conflict"),
        "resource URI should use the merge-conflict scheme"
    );
    assert!(
        resource_uri.contains("main.rs"),
        "resource URI should reference the file path"
    );
}

#[test]
fn test_build_conflict_resolution_prompt_multiple_conflicts_same_file() {
    let conflicts = vec![
        ConflictContent {
            file_path: "src/lib.rs".to_string(),
            conflict_text: "<<<<<<< main\nfn a() {}\n=======\nfn a_v2() {}\n>>>>>>> dev"
                .to_string(),
            ours_branch_name: "main".to_string(),
            theirs_branch_name: "dev".to_string(),
        },
        ConflictContent {
            file_path: "src/lib.rs".to_string(),
            conflict_text: "<<<<<<< main\nfn b() {}\n=======\nfn b_v2() {}\n>>>>>>> dev"
                .to_string(),
            ours_branch_name: "main".to_string(),
            theirs_branch_name: "dev".to_string(),
        },
    ];

    let blocks = build_conflict_resolution_prompt(&conflicts);
    assert_eq!(blocks.len(), 3, "expected 1 text + 2 resource blocks");

    let text = expect_text_block(&blocks[0]);
    assert!(
        text.contains("all 2 merge conflicts"),
        "prompt should mention the total count"
    );
    assert!(
        text.contains("`main` (ours)"),
        "prompt should mention ours branch"
    );
    assert!(
        text.contains("`dev` (theirs)"),
        "prompt should mention theirs branch"
    );
    assert!(
        text.contains("file directly"),
        "single file should use singular 'file'"
    );

    let (resource_a, _) = expect_resource_block(&blocks[1]);
    let (resource_b, _) = expect_resource_block(&blocks[2]);
    assert!(
        resource_a.contains("fn a()"),
        "first resource should contain first conflict"
    );
    assert!(
        resource_b.contains("fn b()"),
        "second resource should contain second conflict"
    );
}

#[test]
fn test_build_conflict_resolution_prompt_multiple_conflicts_different_files() {
    let conflicts = vec![
        ConflictContent {
            file_path: "src/a.rs".to_string(),
            conflict_text: "<<<<<<< main\nA\n=======\nB\n>>>>>>> dev".to_string(),
            ours_branch_name: "main".to_string(),
            theirs_branch_name: "dev".to_string(),
        },
        ConflictContent {
            file_path: "src/b.rs".to_string(),
            conflict_text: "<<<<<<< main\nC\n=======\nD\n>>>>>>> dev".to_string(),
            ours_branch_name: "main".to_string(),
            theirs_branch_name: "dev".to_string(),
        },
    ];

    let blocks = build_conflict_resolution_prompt(&conflicts);
    assert_eq!(blocks.len(), 3, "expected 1 text + 2 resource blocks");

    let text = expect_text_block(&blocks[0]);
    assert!(
        text.contains("files directly"),
        "multiple files should use plural 'files'"
    );

    let (_, uri_a) = expect_resource_block(&blocks[1]);
    let (_, uri_b) = expect_resource_block(&blocks[2]);
    assert!(
        uri_a.contains("a.rs"),
        "first resource URI should reference a.rs"
    );
    assert!(
        uri_b.contains("b.rs"),
        "second resource URI should reference b.rs"
    );
}

#[test]
fn test_build_conflicted_files_resolution_prompt_file_paths_only() {
    let file_paths = vec![
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "tests/integration.rs".to_string(),
    ];

    let blocks = build_conflicted_files_resolution_prompt(&file_paths);
    assert_eq!(
        blocks.len(),
        1 + (file_paths.len() * 2),
        "expected instruction text plus resource links and separators"
    );

    let text = expect_text_block(&blocks[0]);
    assert!(
        text.contains("unresolved merge conflicts"),
        "prompt should describe the task"
    );
    assert!(
        text.contains("conflict markers"),
        "prompt should mention conflict markers"
    );

    for (index, path) in file_paths.iter().enumerate() {
        let link_index = 1 + (index * 2);
        let newline_index = link_index + 1;

        match &blocks[link_index] {
            acp::ContentBlock::ResourceLink(link) => {
                assert!(
                    link.uri.contains("file://"),
                    "resource link URI should use file scheme"
                );
                assert!(
                    link.uri.contains(path),
                    "resource link URI should reference file path: {path}"
                );
            }
            other => panic!(
                "expected ResourceLink block at index {}, got {:?}",
                link_index, other
            ),
        }

        let separator = expect_text_block(&blocks[newline_index]);
        assert_eq!(
            separator, "\n",
            "expected newline separator after each file"
        );
    }
}

#[test]
fn test_build_conflict_resolution_prompt_empty_conflicts() {
    let blocks = build_conflict_resolution_prompt(&[]);
    assert!(
        blocks.is_empty(),
        "empty conflicts should produce no blocks, got {} blocks",
        blocks.len()
    );
}

#[test]
fn test_build_conflicted_files_resolution_prompt_empty_paths() {
    let blocks = build_conflicted_files_resolution_prompt(&[]);
    assert!(
        blocks.is_empty(),
        "empty paths should produce no blocks, got {} blocks",
        blocks.len()
    );
}

#[test]
fn test_conflict_resource_block_structure() {
    let conflict = ConflictContent {
        file_path: "src/utils.rs".to_string(),
        conflict_text: "<<<<<<< HEAD\nold code\n=======\nnew code\n>>>>>>> branch".to_string(),
        ours_branch_name: "HEAD".to_string(),
        theirs_branch_name: "branch".to_string(),
    };

    let block = conflict_resource_block(&conflict);
    let (text, uri) = expect_resource_block(&block);

    assert_eq!(
        text, conflict.conflict_text,
        "resource text should be the raw conflict"
    );
    assert!(
        uri.starts_with("mav:///agent/merge-conflict"),
        "URI should use the mav merge-conflict scheme, got: {uri}"
    );
    assert!(uri.contains("utils.rs"), "URI should encode the file path");
}

use super::*;
use indoc::indoc;

#[test]
fn parse_lines_simple() {
    let input = indoc! {"
            diff --git a/text.txt b/text.txt
            index 86c770d..a1fd855 100644
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,3 @@
             context
            -deleted
            +inserted
            garbage

            --- b/file.txt
            +++ a/file.txt
        "};

    let lines = input.lines().map(DiffLine::parse).collect::<Vec<_>>();

    assert_eq!(
        lines,
        &[
            DiffLine::Garbage("diff --git a/text.txt b/text.txt"),
            DiffLine::Garbage("index 86c770d..a1fd855 100644"),
            DiffLine::OldPath {
                path: "file.txt".into()
            },
            DiffLine::NewPath {
                path: "file.txt".into()
            },
            DiffLine::HunkHeader(Some(HunkLocation {
                start_line_old: 0,
                count_old: 2,
                start_line_new: 0,
                count_new: 3
            })),
            DiffLine::Context("context"),
            DiffLine::Deletion("deleted"),
            DiffLine::Addition("inserted"),
            DiffLine::Garbage("garbage"),
            DiffLine::Context(""),
            DiffLine::OldPath {
                path: "b/file.txt".into()
            },
            DiffLine::NewPath {
                path: "a/file.txt".into()
            },
        ]
    );
}

#[test]
fn file_header_extra_space() {
    let options = ["--- file", "---   file", "---\tfile"];

    for option in options {
        assert_eq!(
            DiffLine::parse(option),
            DiffLine::OldPath {
                path: "file".into()
            },
            "{option}",
        );
    }
}

#[test]
fn hunk_header_extra_space() {
    let options = [
        "@@ -1,2 +1,3 @@",
        "@@  -1,2  +1,3 @@",
        "@@\t-1,2\t+1,3\t@@",
        "@@ -1,2  +1,3 @@",
        "@@ -1,2   +1,3 @@",
        "@@ -1,2 +1,3   @@",
        "@@ -1,2 +1,3 @@ garbage",
    ];

    for option in options {
        assert_eq!(
            DiffLine::parse(option),
            DiffLine::HunkHeader(Some(HunkLocation {
                start_line_old: 0,
                count_old: 2,
                start_line_new: 0,
                count_new: 3
            })),
            "{option}",
        );
    }
}

#[test]
fn hunk_header_without_location() {
    assert_eq!(DiffLine::parse("@@ ... @@"), DiffLine::HunkHeader(None));
}

#[test]
fn test_parse_path() {
    assert_eq!(parse_header_path("a/", "foo.txt"), "foo.txt");
    assert_eq!(
        parse_header_path("a/", "foo/bar/baz.txt"),
        "foo/bar/baz.txt"
    );
    assert_eq!(parse_header_path("a/", "a/foo.txt"), "foo.txt");
    assert_eq!(
        parse_header_path("a/", "a/foo/bar/baz.txt"),
        "foo/bar/baz.txt"
    );

    // Extra
    assert_eq!(
        parse_header_path("a/", "a/foo/bar/baz.txt  2025"),
        "foo/bar/baz.txt"
    );
    assert_eq!(
        parse_header_path("a/", "a/foo/bar/baz.txt\t2025"),
        "foo/bar/baz.txt"
    );
    assert_eq!(
        parse_header_path("a/", "a/foo/bar/baz.txt \""),
        "foo/bar/baz.txt"
    );

    // Quoted
    assert_eq!(
        parse_header_path("a/", "a/foo/bar/\"baz quox.txt\""),
        "foo/bar/baz quox.txt"
    );
    assert_eq!(
        parse_header_path("a/", "\"a/foo/bar/baz quox.txt\""),
        "foo/bar/baz quox.txt"
    );
    assert_eq!(
        parse_header_path("a/", "\"foo/bar/baz quox.txt\""),
        "foo/bar/baz quox.txt"
    );
    assert_eq!(parse_header_path("a/", "\"whatever 🤷\""), "whatever 🤷");
    assert_eq!(
        parse_header_path("a/", "\"foo/bar/baz quox.txt\"  2025"),
        "foo/bar/baz quox.txt"
    );
    // unescaped quotes are dropped
    assert_eq!(parse_header_path("a/", "foo/\"bar\""), "foo/bar");

    // Escaped
    assert_eq!(
        parse_header_path("a/", "\"foo/\\\"bar\\\"/baz.txt\""),
        "foo/\"bar\"/baz.txt"
    );
    assert_eq!(
        parse_header_path("a/", "\"C:\\\\Projects\\\\My App\\\\old file.txt\""),
        "C:\\Projects\\My App\\old file.txt"
    );
}

#[test]
fn test_parse_diff_with_leading_and_trailing_garbage() {
    let diff = indoc! {"
            I need to make some changes.

            I'll change the following things:
            - one
              - two
            - three

            ```
            --- a/file.txt
            +++ b/file.txt
             one
            +AND
             two
            ```

            Summary of what I did:
            - one
              - two
            - three

            That's about it.
        "};

    let mut events = Vec::new();
    let mut parser = DiffParser::new(diff);
    while let Some(event) = parser.next().unwrap() {
        events.push(event);
    }

    assert_eq!(
        events,
        &[
            DiffEvent::Hunk {
                path: "file.txt".into(),
                hunk: Hunk {
                    context: "one\ntwo\n".into(),
                    edits: vec![Edit {
                        range: 4..4,
                        text: "AND\n".into()
                    }],
                    start_line: None,
                },
                status: FileStatus::Modified,
            },
            DiffEvent::FileEnd { renamed_to: None }
        ],
    )
}

#[test]
fn test_no_newline_at_eof() {
    let diff = indoc! {"
            --- a/file.py
            +++ b/file.py
            @@ -55,7 +55,3 @@ class CustomDataset(Dataset):
                         torch.set_rng_state(state)
                         mask = self.transform(mask)

            -        if self.mode == 'Training':
            -            return (img, mask, name)
            -        else:
            -            return (img, mask, name)
            \\ No newline at end of file
        "};

    let mut events = Vec::new();
    let mut parser = DiffParser::new(diff);
    while let Some(event) = parser.next().unwrap() {
        events.push(event);
    }

    assert_eq!(
        events,
        &[
            DiffEvent::Hunk {
                path: "file.py".into(),
                hunk: Hunk {
                    context: concat!(
                        "            torch.set_rng_state(state)\n",
                        "            mask = self.transform(mask)\n",
                        "\n",
                        "        if self.mode == 'Training':\n",
                        "            return (img, mask, name)\n",
                        "        else:\n",
                        "            return (img, mask, name)",
                    )
                    .into(),
                    edits: vec![Edit {
                        range: 80..203,
                        text: "".into()
                    }],
                    start_line: Some(54), // @@ -55,7 -> line 54 (0-indexed)
                },
                status: FileStatus::Modified,
            },
            DiffEvent::FileEnd { renamed_to: None }
        ],
    );
}

#[test]
fn test_no_newline_at_eof_addition() {
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,3 @@
             context
            -deleted
            +added line
            \\ No newline at end of file
        "};

    let mut events = Vec::new();
    let mut parser = DiffParser::new(diff);
    while let Some(event) = parser.next().unwrap() {
        events.push(event);
    }

    assert_eq!(
        events,
        &[
            DiffEvent::Hunk {
                path: "file.txt".into(),
                hunk: Hunk {
                    context: "context\ndeleted\n".into(),
                    edits: vec![Edit {
                        range: 8..16,
                        text: "added line".into()
                    }],
                    start_line: Some(0), // @@ -1,2 -> line 0 (0-indexed)
                },
                status: FileStatus::Modified,
            },
            DiffEvent::FileEnd { renamed_to: None }
        ],
    );
}

#[test]
fn test_double_no_newline_at_eof() {
    // Two consecutive "no newline" markers - the second should be ignored
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,3 @@
             line1
            -old
            +new
             line3
            \\ No newline at end of file
            \\ No newline at end of file
        "};

    let mut events = Vec::new();
    let mut parser = DiffParser::new(diff);
    while let Some(event) = parser.next().unwrap() {
        events.push(event);
    }

    assert_eq!(
        events,
        &[
            DiffEvent::Hunk {
                path: "file.txt".into(),
                hunk: Hunk {
                    context: "line1\nold\nline3".into(), // Only one newline removed
                    edits: vec![Edit {
                        range: 6..10, // "old\n" is 4 bytes
                        text: "new\n".into()
                    }],
                    start_line: Some(0),
                },
                status: FileStatus::Modified,
            },
            DiffEvent::FileEnd { renamed_to: None }
        ],
    );
}

#[test]
fn test_no_newline_after_context_not_addition() {
    // "No newline" after context lines should remove newline from context,
    // not from an earlier addition
    let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,4 +1,4 @@
             line1
            -old
            +new
             line3
             line4
            \\ No newline at end of file
        "};

    let mut events = Vec::new();
    let mut parser = DiffParser::new(diff);
    while let Some(event) = parser.next().unwrap() {
        events.push(event);
    }

    assert_eq!(
        events,
        &[
            DiffEvent::Hunk {
                path: "file.txt".into(),
                hunk: Hunk {
                    // newline removed from line4 (context), not from "new" (addition)
                    context: "line1\nold\nline3\nline4".into(),
                    edits: vec![Edit {
                        range: 6..10,         // "old\n" is 4 bytes
                        text: "new\n".into()  // Still has newline
                    }],
                    start_line: Some(0),
                },
                status: FileStatus::Modified,
            },
            DiffEvent::FileEnd { renamed_to: None }
        ],
    );
}

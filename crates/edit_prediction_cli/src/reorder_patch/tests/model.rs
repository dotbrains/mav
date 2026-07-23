use super::*;
use indoc::indoc;

use super::*;
use indoc::indoc;

#[test]
fn test_parse_unified_diff() {
    let patch_str = indoc! {"
        Patch header
        ============

        diff --git a/text.txt b/text.txt
        index 86c770d..a1fd855 100644
        --- a/text.txt
        +++ b/text.txt
        @@ -1,7 +1,7 @@
         azuere
         beige
         black
        -blue
        +dark blue
         brown
         cyan
         gold

        Some garbage

        diff --git a/second.txt b/second.txt
        index 86c770d..a1fd855 100644
        --- a/second.txt
        +++ b/second.txt
        @@ -9,6 +9,7 @@ gray
         green
         indigo
         magenta
        +silver
         orange
         pink
         purple
        diff --git a/text.txt b/text.txt
        index 86c770d..a1fd855 100644
        --- a/text.txt
        +++ b/text.txt
        @@ -16,4 +17,3 @@ red
         violet
         white
         yellow
        -zinc
    "};
    let patch = Patch::parse_unified_diff(patch_str);

    assert_eq!(patch.header, "Patch header\n============\n\n");
    assert_eq!(patch.hunks.len(), 3);
    assert_eq!(patch.hunks[0].header_string(), "@@ -1,7 +1,7 @@");
    assert_eq!(patch.hunks[1].header_string(), "@@ -9,6 +9,7 @@ gray");
    assert_eq!(patch.hunks[2].header_string(), "@@ -16,4 +17,3 @@ red");
    assert_eq!(patch.hunks[0].is_filename_inherited, false);
    assert_eq!(patch.hunks[1].is_filename_inherited, false);
    assert_eq!(patch.hunks[2].is_filename_inherited, false);
}

#[test]
fn test_locate_edited_line() {
    let patch_str = indoc! {"
        Patch header
        ============

        diff --git a/text.txt b/text.txt
        index 86c770d..a1fd855 100644
        --- a/text.txt
        +++ b/text.txt
        @@ -1,7 +1,7 @@
         azuere
         beige
         black
        -blue
        +dark blue
         brown
         cyan
         gold
        diff --git a/second.txt b/second.txt
        index 86c770d..a1fd855 100644
        --- a/second.txt
        +++ b/second.txt
        @@ -9,6 +9,7 @@ gray
         green
         indigo
         magenta
        +silver
         orange
         pink
         purple
        diff --git a/text.txt b/text.txt
        index 86c770d..a1fd855 100644
        --- a/text.txt
        +++ b/text.txt
        @@ -16,4 +17,3 @@ red
         violet
         white
         yellow
        -zinc
    "};
    let patch = Patch::parse_unified_diff(patch_str);
    let locations = edit_locations(&patch);
    assert_eq!(locations.len(), 4);

    assert_eq!(
        locate_edited_line(&patch, 0), // -blue
        Some(EditLocation {
            filename: "text.txt".to_string(),
            source_line_number: 4,
            target_line_number: 4,
            patch_line: PatchLine::Deletion("blue".to_string()),
            hunk_index: 0,
            line_index_within_hunk: 3
        })
    );
    assert_eq!(
        locate_edited_line(&patch, 1), // +dark blue
        Some(EditLocation {
            filename: "text.txt".to_string(),
            source_line_number: 5,
            target_line_number: 4,
            patch_line: PatchLine::Addition("dark blue".to_string()),
            hunk_index: 0,
            line_index_within_hunk: 4
        })
    );
    assert_eq!(
        locate_edited_line(&patch, 2), // +silver
        Some(EditLocation {
            filename: "second.txt".to_string(),
            source_line_number: 12,
            target_line_number: 12,
            patch_line: PatchLine::Addition("silver".to_string()),
            hunk_index: 1,
            line_index_within_hunk: 3
        })
    );
}

#[test]
fn test_normalize_hunk() {
    let mut patch = Patch::parse_unified_diff(indoc! {"
        This patch has too many lines of context.

        --- a/first.txt
        +++ b/first.txt
        @@ -1,7 +1,6 @@
         azuere
         beige
         black
        -blue
         brown
         cyan
         gold
        // Some garbage
    "});

    patch.normalize_hunks(1);
    let actual = patch.to_string();
    assert_eq!(
        actual,
        indoc! {"
        This patch has too many lines of context.

        --- a/first.txt
        +++ b/first.txt
        @@ -3,3 +3,2 @@
         black
        -blue
         brown
        // Some garbage
    "}
    );
}

#[test]
fn test_file_creation_diff_header() {
    // When old_start and old_count are both 0, the file is being created,
    // so the --- line should be /dev/null instead of a/filename
    let patch = Patch::parse_unified_diff(indoc! {"
        --- a/new_file.rs
        +++ b/new_file.rs
        @@ -0,0 +1,3 @@
        +fn main() {
        +    println!(\"hello\");
        +}
    "});

    let actual = patch.to_string();
    assert_eq!(
        actual,
        indoc! {"
        --- /dev/null
        +++ b/new_file.rs
        @@ -0,0 +1,3 @@
        +fn main() {
        +    println!(\"hello\");
        +}
    "}
    );
}

#[test]
fn test_file_deletion_diff_header() {
    // When new_start and new_count are both 0, the file is being deleted,
    // so the +++ line should be /dev/null instead of b/filename
    let patch = Patch::parse_unified_diff(indoc! {"
        --- a/old_file.rs
        +++ /dev/null
        @@ -1,3 +0,0 @@
        -fn main() {
        -    println!(\"goodbye\");
        -}
    "});

    let actual = patch.to_string();
    assert_eq!(
        actual,
        indoc! {"
        --- a/old_file.rs
        +++ /dev/null
        @@ -1,3 +0,0 @@
        -fn main() {
        -    println!(\"goodbye\");
        -}
    "}
    );
}

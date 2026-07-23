use super::*;
use indoc::indoc;
use pretty_assertions::assert_eq;

mod remove_edits {
    use super::*;
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    static PATCH: &'static str = indoc! {"
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
        @@ -9,6 +9,7 @@ gray
         green
         indigo
         magenta
        +silver
         orange
         pink
         purple
        @@ -16,4 +17,3 @@ red
         violet
         white
         yellow
        -zinc
    "};

    #[test]
    fn test_removes_hunks_without_edits() {
        // Remove the first two edits:
        // -blue
        // +dark blue
        let mut patch = Patch::parse_unified_diff(PATCH);
        remove_edits(&mut patch, vec![0, 1]);

        // The whole hunk should be removed since there are no other edits in it
        let actual = patch.to_string();
        let expected = indoc! {"
            --- a/text.txt
            +++ b/text.txt
            @@ -9,6 +9,7 @@ gray
             green
             indigo
             magenta
            +silver
             orange
             pink
             purple
            --- a/text.txt
            +++ b/text.txt
            @@ -16,4 +17,3 @@ red
             violet
             white
             yellow
            -zinc
        "};
        assert_eq!(actual, String::from(expected));
    }

    #[test]
    fn test_adjust_line_numbers_after_deletion() {
        // Remove the first deletion (`-blue`)
        let mut patch = Patch::parse_unified_diff(PATCH);
        remove_edits(&mut patch, vec![0]);

        // The line numbers should be adjusted in the subsequent hunks
        println!("{}", &patch.to_string());
        assert_eq!(patch.hunks[0].header_string(), "@@ -2,6 +2,7 @@");
        assert_eq!(patch.hunks[1].header_string(), "@@ -9,6 +10,7 @@ gray");
        assert_eq!(patch.hunks[2].header_string(), "@@ -16,4 +18,3 @@ red");
    }
    #[test]
    fn test_adjust_line_numbers_after_insertion() {
        // Remove the first insertion (`+dark blue`)
        let mut patch = Patch::parse_unified_diff(PATCH);
        remove_edits(&mut patch, vec![1]);

        // The line numbers should be adjusted in the subsequent hunks
        assert_eq!(patch.hunks[0].header_string(), "@@ -1,7 +1,6 @@");
        assert_eq!(patch.hunks[1].header_string(), "@@ -9,6 +8,7 @@ gray");
        assert_eq!(patch.hunks[2].header_string(), "@@ -16,4 +16,3 @@ red");
    }
    #[test]
    fn test_adjust_line_numbers_multifile_case() {
        // Given a patch that spans multiple files
        let patch_str = indoc! {"
            --- a/first.txt
            +++ b/first.txt
            @@ -1,7 +1,7 @@
             azuere
             beige
             black
            -blue
            +dark blue
             brown
             cyan
             gold
            @@ -16,4 +17,3 @@ red
             violet
             white
             yellow
            -zinc
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
        "};

        // When removing edit from one of the files (`+dark blue`)
        let mut patch = Patch::parse_unified_diff(patch_str);
        remove_edits(&mut patch, vec![1]);

        // Then the line numbers should only be adjusted in subsequent hunks from that file
        assert_eq!(patch.hunks[0].header_string(), "@@ -1,7 +1,6 @@"); // edited hunk
        assert_eq!(patch.hunks[1].header_string(), "@@ -16,4 +16,3 @@ red"); // hunk from edited file again
        assert_eq!(patch.hunks[2].header_string(), "@@ -9,6 +9,7 @@ gray"); // hunk from another file

        // When removing hunk from `second.txt`
        let mut patch = Patch::parse_unified_diff(patch_str);
        remove_edits(&mut patch, vec![3]);

        // Then patch serialization should list `first.txt` only once
        // (because hunks from that file become adjacent)
        let expected = indoc! {"
            --- a/first.txt
            +++ b/first.txt
            @@ -1,7 +1,7 @@
             azuere
             beige
             black
            -blue
            +dark blue
             brown
             cyan
             gold
            --- a/first.txt
            +++ b/first.txt
            @@ -16,4 +17,3 @@ red
             violet
             white
             yellow
            -zinc
        "};
        assert_eq!(patch.to_string(), expected);
    }

    #[test]
    fn test_dont_adjust_line_numbers_samefile_case() {
        // Given a patch that has hunks in the same file, but with a file header
        // (which makes `git apply` flush edits so far and start counting lines numbers afresh)
        let patch_str = indoc! {"
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
            --- a/text.txt
            +++ b/text.txt
            @@ -16,4 +16,3 @@ red
             violet
             white
             yellow
            -zinc
    "};

        // When removing edit from one of the files (`+dark blue`)
        let mut patch = Patch::parse_unified_diff(patch_str);
        remove_edits(&mut patch, vec![1]);

        // Then the line numbers should **not** be adjusted in a subsequent hunk,
        // because it starts with a file header
        assert_eq!(patch.hunks[0].header_string(), "@@ -1,7 +1,6 @@"); // edited hunk
        assert_eq!(patch.hunks[1].header_string(), "@@ -16,4 +16,3 @@ red"); // subsequent hunk
    }
}

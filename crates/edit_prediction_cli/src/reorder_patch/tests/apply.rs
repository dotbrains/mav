use super::*;
use indoc::indoc;
use pretty_assertions::assert_eq;

mod apply_edits {
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

    #[test]
    fn test_removes_hunks_without_edits() {
        // When applying the first two edits (`-blue`, `+dark blue`)
        let mut patch = Patch::parse_unified_diff(PATCH);
        apply_edits(&mut patch, vec![0, 1]);

        // Then the whole hunk should be removed since there are no other edits in it,
        // and the line numbers should be adjusted in the subsequent hunks
        assert_eq!(patch.hunks[0].header_string(), "@@ -9,6 +9,7 @@ gray");
        assert_eq!(patch.hunks[1].header_string(), "@@ -16,4 +17,3 @@ red");
        assert_eq!(patch.hunks.len(), 2);
    }

    #[test]
    fn test_adjust_line_numbers_after_applying_deletion() {
        // Apply the first deletion (`-blue`)
        let mut patch = Patch::parse_unified_diff(PATCH);
        apply_edits(&mut patch, vec![0]);

        // The line numbers should be adjusted
        assert_eq!(patch.hunks[0].header_string(), "@@ -1,6 +1,7 @@");
        assert_eq!(patch.hunks[1].header_string(), "@@ -8,6 +9,7 @@ gray");
        assert_eq!(patch.hunks[2].header_string(), "@@ -15,4 +17,3 @@ red");
    }
    #[test]
    fn test_adjust_line_numbers_after_applying_insertion() {
        // Apply the first insertion (`+dark blue`)
        let mut patch = Patch::parse_unified_diff(PATCH);
        apply_edits(&mut patch, vec![1]);

        // The line numbers should be adjusted in the subsequent hunks
        println!("{}", &patch.to_string());
        assert_eq!(patch.hunks[0].header_string(), "@@ -1,7 +1,6 @@");
        assert_eq!(patch.hunks[1].header_string(), "@@ -10,6 +9,7 @@ gray");
        assert_eq!(patch.hunks[2].header_string(), "@@ -17,4 +17,3 @@ red");
    }
}

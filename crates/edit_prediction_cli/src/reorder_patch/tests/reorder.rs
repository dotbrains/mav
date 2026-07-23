use super::*;
use indoc::indoc;
use pretty_assertions::assert_eq;

mod reorder_edits {
    use super::*;
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    static PATCH: &'static str = indoc! {"
        Some header.

        diff --git a/first.txt b/first.txt
        index 86c770d..a1fd855 100644
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
        --- a/first.txt
        +++ b/first.txt
        @@ -16,4 +17,3 @@ red
         violet
         white
         yellow
        -zinc
    "};

    #[test]
    fn test_reorder_1() {
        let edits_order = vec![
            BTreeSet::from([2]),    // +silver
            BTreeSet::from([3]),    // -zinc
            BTreeSet::from([0, 1]), // -blue +dark blue
        ];

        let patch = Patch::parse_unified_diff(PATCH);
        let reordered_patch = reorder_edits(&patch, edits_order);

        // The whole hunk should be removed since there are no other edits in it
        let actual = reordered_patch.to_string();

        println!("{}", actual);

        let expected = indoc! {"
           Some header.

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
           --- a/first.txt
           +++ b/first.txt
           @@ -16,4 +17,3 @@ red
            violet
            white
            yellow
           -zinc
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
        "};
        assert_eq!(actual, String::from(expected));
    }

    #[test]
    fn test_reorder_duplicates() {
        let edits_order = vec![
            BTreeSet::from([2]), // +silver
            BTreeSet::from([2]), // +silver again
            BTreeSet::from([3]), // -zinc
        ];

        let patch = Patch::parse_unified_diff(PATCH);
        let reordered_patch = reorder_edits(&patch, edits_order);

        // The whole hunk should be removed since there are no other edits in it
        let actual = reordered_patch.to_string();

        println!("{}", actual);

        let expected = indoc! {"
                   Some header.

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
                   --- a/first.txt
                   +++ b/first.txt
                   @@ -16,4 +17,3 @@ red
                    violet
                    white
                    yellow
                   -zinc
                "};
        assert_eq!(actual, String::from(expected));
    }
}

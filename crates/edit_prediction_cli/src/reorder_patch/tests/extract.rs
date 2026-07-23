use super::*;
use indoc::indoc;
use pretty_assertions::assert_eq;

mod extract_edits {

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

    #[test]
    fn test_extract_edits() {
        let to_extract = BTreeSet::from([
            3, // +silver
            0, // -blue
        ]);

        let mut patch = Patch::parse_unified_diff(PATCH);
        let (extracted, remainder) = extract_edits(&mut patch, &to_extract);

        // Edits will be extracted in the sorted order, so [0, 3]
        let expected_extracted = indoc! {"
           Some header.

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

        let expected_remainder = indoc! {"
            Some header.

            --- a/first.txt
            +++ b/first.txt
            @@ -1,6 +1,7 @@
             azuere
             beige
             black
            +dark blue
             brown
             cyan
             gold
            --- a/first.txt
            +++ b/first.txt
            @@ -15,4 +17,3 @@ red
             violet
             white
             yellow
            -zinc
        "};
        assert_eq!(extracted.to_string(), String::from(expected_extracted));
        assert_eq!(remainder.to_string(), String::from(expected_remainder));
    }
}

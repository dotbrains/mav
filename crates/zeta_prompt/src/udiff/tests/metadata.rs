use super::*;
use indoc::indoc;

#[test]
fn test_strip_diff_metadata() {
    let diff_with_metadata = indoc! {r#"
            diff --git a/file.txt b/file.txt
            index 1234567..abcdefg 100644
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,4 @@
             context line
            -removed line
            +added line
             more context
        "#};

    let stripped = strip_diff_metadata(diff_with_metadata);

    assert_eq!(
        stripped,
        indoc! {r#"
                --- a/file.txt
                +++ b/file.txt
                @@ -1,3 +1,4 @@
                 context line
                -removed line
                +added line
                 more context
            "#}
    );
}

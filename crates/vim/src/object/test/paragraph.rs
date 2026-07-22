use super::*;

const PARAGRAPH_EXAMPLES: &[&str] = &[
    // Single line
    "ˇThe quick brown fox jumpˇs over the lazy dogˇ.ˇ",
    // Multiple lines without empty lines
    indoc! {"
            ˇThe quick brownˇ
            ˇfox jumps overˇ
            the lazy dog.ˇ
        "},
    // Heading blank paragraph and trailing normal paragraph
    indoc! {"
            ˇ
            ˇ
            ˇThe quick brown fox jumps
            ˇover the lazy dog.
            ˇ
            ˇ
            ˇThe quick brown fox jumpsˇ
            ˇover the lazy dog.ˇ
        "},
    // Inserted blank paragraph and trailing blank paragraph
    indoc! {"
            ˇThe quick brown fox jumps
            ˇover the lazy dog.
            ˇ
            ˇ
            ˇ
            ˇThe quick brown fox jumpsˇ
            ˇover the lazy dog.ˇ
            ˇ
            ˇ
            ˇ
        "},
    // "Blank" paragraph with whitespace characters
    indoc! {"
            ˇThe quick brown fox jumps
            over the lazy dog.

            ˇ \t

            ˇThe quick brown fox jumps
            over the lazy dog.ˇ
            ˇ
            ˇ \t
            \t \t
        "},
    // Single line "paragraphs", where selection size might be zero.
    indoc! {"
            ˇThe quick brown fox jumps over the lazy dog.
            ˇ
            ˇThe quick brown fox jumpˇs over the lazy dog.ˇ
            ˇ
        "},
];

#[gpui::test]
async fn test_change_paragraph_object(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for paragraph_example in PARAGRAPH_EXAMPLES {
        cx.simulate_at_each_offset("c i p", paragraph_example)
            .await
            .assert_matches();
        cx.simulate_at_each_offset("c a p", paragraph_example)
            .await
            .assert_matches();
    }
}

#[gpui::test]
async fn test_delete_paragraph_object(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for paragraph_example in PARAGRAPH_EXAMPLES {
        cx.simulate_at_each_offset("d i p", paragraph_example)
            .await
            .assert_matches();
        cx.simulate_at_each_offset("d a p", paragraph_example)
            .await
            .assert_matches();
    }
}

#[gpui::test]
async fn test_visual_paragraph_object(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    const EXAMPLES: &[&str] = &[
        indoc! {"
                ˇThe quick brown
                fox jumps over
                the lazy dog.
            "},
        indoc! {"
                ˇ

                ˇThe quick brown fox jumps
                over the lazy dog.
                ˇ

                ˇThe quick brown fox jumps
                over the lazy dog.
            "},
        indoc! {"
                ˇThe quick brown fox jumps over the lazy dog.
                ˇ
                ˇThe quick brown fox jumps over the lazy dog.

            "},
    ];

    for paragraph_example in EXAMPLES {
        cx.simulate_at_each_offset("v i p", paragraph_example)
            .await
            .assert_matches();
        cx.simulate_at_each_offset("v a p", paragraph_example)
            .await
            .assert_matches();
    }
}

#[gpui::test]
async fn test_change_paragraph_object_with_soft_wrap(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    const WRAPPING_EXAMPLE: &str = indoc! {"
            ˇFirst paragraph with very long text that will wrap when soft wrap is enabled and line length is ˇlimited making it span multiple display lines.

            ˇSecond paragraph that is also quite long and will definitely wrap under soft wrap conditions and ˇshould be handled correctly.

            ˇThird paragraph with additional long text content that will also wrap when line length is constrained by the wrapping ˇsettings.ˇ
        "};

    cx.set_shared_wrap(20).await;

    cx.simulate_at_each_offset("c i p", WRAPPING_EXAMPLE)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("c a p", WRAPPING_EXAMPLE)
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_delete_paragraph_object_with_soft_wrap(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    const WRAPPING_EXAMPLE: &str = indoc! {"
            ˇFirst paragraph with very long text that will wrap when soft wrap is enabled and line length is ˇlimited making it span multiple display lines.

            ˇSecond paragraph that is also quite long and will definitely wrap under soft wrap conditions and ˇshould be handled correctly.

            ˇThird paragraph with additional long text content that will also wrap when line length is constrained by the wrapping ˇsettings.ˇ
        "};

    cx.set_shared_wrap(20).await;

    cx.simulate_at_each_offset("d i p", WRAPPING_EXAMPLE)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("d a p", WRAPPING_EXAMPLE)
        .await
        .assert_matches();
}

#[gpui::test]
async fn test_delete_paragraph_whitespace(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {"
            a
                   ˇ•
            aaaaaaaaaaaaa
        "})
        .await;

    cx.simulate_shared_keystrokes("d i p").await;
    cx.shared_state().await.assert_eq(indoc! {"
            a
            aaaaaaaˇaaaaaa
        "});
}

#[gpui::test]
async fn test_visual_paragraph_object_with_soft_wrap(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    const WRAPPING_EXAMPLE: &str = indoc! {"
            ˇFirst paragraph with very long text that will wrap when soft wrap is enabled and line length is ˇlimited making it span multiple display lines.

            ˇSecond paragraph that is also quite long and will definitely wrap under soft wrap conditions and ˇshould be handled correctly.

            ˇThird paragraph with additional long text content that will also wrap when line length is constrained by the wrapping ˇsettings.ˇ
        "};

    cx.set_shared_wrap(20).await;

    cx.simulate_at_each_offset("v i p", WRAPPING_EXAMPLE)
        .await
        .assert_matches();
    cx.simulate_at_each_offset("v a p", WRAPPING_EXAMPLE)
        .await
        .assert_matches();
}

// Test string with "`" for opening surrounders and "'" for closing surrounders
const SURROUNDING_MARKER_STRING: &str = indoc! {"
        ˇTh'ˇe ˇ`ˇ'ˇquˇi`ˇck broˇ'wn`
        'ˇfox juˇmps ov`ˇer
        the ˇlazy d'o`ˇg"};

const SURROUNDING_OBJECTS: &[(char, char)] = &[
    ('"', '"'), // Double Quote
    ('(', ')'), // Parentheses
];

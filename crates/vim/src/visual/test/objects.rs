use super::*;

#[gpui::test]
async fn test_visual_object(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("hello (in [parˇens] o)").await;
    cx.simulate_shared_keystrokes("ctrl-v l").await;
    cx.simulate_shared_keystrokes("a ]").await;
    cx.shared_state()
        .await
        .assert_eq("hello (in «[parens]ˇ» o)");
    cx.simulate_shared_keystrokes("i (").await;
    cx.shared_state()
        .await
        .assert_eq("hello («in [parens] oˇ»)");

    cx.set_shared_state("hello in a wˇord again.").await;
    cx.simulate_shared_keystrokes("ctrl-v l i w").await;
    cx.shared_state()
        .await
        .assert_eq("hello in a w«ordˇ» again.");
    assert_eq!(cx.mode(), Mode::VisualBlock);
    cx.simulate_shared_keystrokes("o a s").await;
    cx.shared_state()
        .await
        .assert_eq("«ˇhello in a word» again.");
}

#[gpui::test]
async fn test_visual_object_expands(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state(indoc! {
        "{
            {
           ˇ }
        }
        {
        }
        "
    })
    .await;
    cx.simulate_shared_keystrokes("v l").await;
    cx.shared_state().await.assert_eq(indoc! {
        "{
            {
           « }ˇ»
        }
        {
        }
        "
    });
    cx.simulate_shared_keystrokes("a {").await;
    cx.shared_state().await.assert_eq(indoc! {
        "{
            «{
            }ˇ»
        }
        {
        }
        "
    });
    cx.simulate_shared_keystrokes("a {").await;
    cx.shared_state().await.assert_eq(indoc! {
        "«{
            {
            }
        }ˇ»
        {
        }
        "
    });
    // cx.simulate_shared_keystrokes("a {").await;
    // cx.shared_state().await.assert_eq(indoc! {
    //     "{
    //         «{
    //         }ˇ»
    //     }
    //     {
    //     }
    //     "
    // });
}

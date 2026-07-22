use super::*;

#[gpui::test]
async fn test_change_surrounding_character_objects(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    for (start, end) in SURROUNDING_OBJECTS {
        let marked_string = SURROUNDING_MARKER_STRING
            .replace('`', &start.to_string())
            .replace('\'', &end.to_string());

        cx.simulate_at_each_offset(&format!("c i {start}"), &marked_string)
            .await
            .assert_matches();
        cx.simulate_at_each_offset(&format!("c i {end}"), &marked_string)
            .await
            .assert_matches();
        cx.simulate_at_each_offset(&format!("c a {start}"), &marked_string)
            .await
            .assert_matches();
        cx.simulate_at_each_offset(&format!("c a {end}"), &marked_string)
            .await
            .assert_matches();
    }
}
#[gpui::test]
async fn test_singleline_surrounding_character_objects(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_wrap(12).await;

    cx.set_shared_state(indoc! {
        "\"ˇhello world\"!"
    })
    .await;
    cx.simulate_shared_keystrokes("v i \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "\"«hello worldˇ»\"!"
    });

    cx.set_shared_state(indoc! {
        "\"hˇello world\"!"
    })
    .await;
    cx.simulate_shared_keystrokes("v i \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "\"«hello worldˇ»\"!"
    });

    cx.set_shared_state(indoc! {
        "helˇlo \"world\"!"
    })
    .await;
    cx.simulate_shared_keystrokes("v i \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "hello \"«worldˇ»\"!"
    });

    cx.set_shared_state(indoc! {
        "hello \"wˇorld\"!"
    })
    .await;
    cx.simulate_shared_keystrokes("v i \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "hello \"«worldˇ»\"!"
    });

    cx.set_shared_state(indoc! {
        "hello \"wˇorld\"!"
    })
    .await;
    cx.simulate_shared_keystrokes("v a \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "hello« \"world\"ˇ»!"
    });

    cx.set_shared_state(indoc! {
        "hello \"wˇorld\" !"
    })
    .await;
    cx.simulate_shared_keystrokes("v a \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "hello «\"world\" ˇ»!"
    });

    cx.set_shared_state(indoc! {
        "hello \"wˇorld\"•
        goodbye"
    })
    .await;
    cx.simulate_shared_keystrokes("v a \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "hello «\"world\" ˇ»
        goodbye"
    });
}

#[gpui::test]
async fn test_multiline_surrounding_character_objects(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    cx.set_state(
        indoc! {
            "func empty(a string) bool {
                   if a == \"\" {
                      return true
                   }
                   ˇreturn false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i {");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                   «if a == \"\" {
                      return true
                   }
                   return falseˇ»
                }"
        },
        Mode::Visual,
    );

    cx.set_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         ˇreturn true
                     }
                     return false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i {");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         «return trueˇ»
                     }
                     return false
                }"
        },
        Mode::Visual,
    );

    cx.set_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" ˇ{
                         return true
                     }
                     return false
                }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i {");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         «return trueˇ»
                     }
                     return false
                }"
        },
        Mode::Visual,
    );

    cx.set_state(
        indoc! {
            "func empty(a string) bool {
                     if a == \"\" {
                         return true
                     }
                     return false
                ˇ}"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("v i {");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                     «if a == \"\" {
                         return true
                     }
                     return falseˇ»
                }"
        },
        Mode::Visual,
    );

    cx.set_state(
        indoc! {
            "func empty(a string) bool {
                             if a == \"\" {
                             ˇ

                             }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("c i {");
    cx.assert_state(
        indoc! {
            "func empty(a string) bool {
                         if a == \"\" {ˇ}"
        },
        Mode::Insert,
    );
}

#[gpui::test]
async fn test_singleline_surrounding_character_objects_with_escape(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;
    cx.set_shared_state(indoc! {
        "h\"e\\\"lˇlo \\\"world\"!"
    })
    .await;
    cx.simulate_shared_keystrokes("v i \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "h\"«e\\\"llo \\\"worldˇ»\"!"
    });

    cx.set_shared_state(indoc! {
        "hello \"teˇst \\\"inside\\\" world\""
    })
    .await;
    cx.simulate_shared_keystrokes("v i \"").await;
    cx.shared_state().await.assert_eq(indoc! {
        "hello \"«test \\\"inside\\\" worldˇ»\""
    });
}

#[gpui::test]
async fn test_vertical_bars(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;
    cx.set_state(
        indoc! {"
            fn boop() {
                baz(ˇ|a, b| { bar(|j, k| { })})
            }"
        },
        Mode::Normal,
    );
    cx.simulate_keystrokes("c i |");
    cx.assert_state(
        indoc! {"
            fn boop() {
                baz(|ˇ| { bar(|j, k| { })})
            }"
        },
        Mode::Insert,
    );
    cx.simulate_keystrokes("escape 1 8 |");
    cx.assert_state(
        indoc! {"
            fn boop() {
                baz(|| { bar(ˇ|j, k| { })})
            }"
        },
        Mode::Normal,
    );

    cx.simulate_keystrokes("v a |");
    cx.assert_state(
        indoc! {"
            fn boop() {
                baz(|| { bar(«|j, k| ˇ»{ })})
            }"
        },
        Mode::Visual,
    );
}

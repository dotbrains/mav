use super::*;

#[gpui::test]
async fn test_addition_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"
        struct Row;
        struct Row1;
        struct Row2;

        struct Row4;
        struct Row5;
        struct Row6;

        struct Row8;
        struct Row9;
        struct Row10;"#};

    // When addition hunks are not adjacent to carets, no hunk revert is performed
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row1.1;
                   struct Row1.2;
                   struct Row2;ˇ

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;
                   ˇstruct Row9;
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Added, DiffHunkStatusKind::Added],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row1.1;
                   struct Row1.2;
                   struct Row2;ˇ

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;
                   ˇstruct Row9;
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    // Same for selections
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row2;
                   struct Row2.1;
                   struct Row2.2;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row8;
                   struct Row9;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Added, DiffHunkStatusKind::Added],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row2;
                   struct Row2.1;
                   struct Row2.2;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row9.1;
                   struct Row9.2;
                   struct Row9.3;
                   struct Row8;
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );

    // When carets and selections intersect the addition hunks, those are reverted.
    // Adjacent carets got merged.
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   ˇ// something on the top
                   struct Row1;
                   struct Row2;
                   struct Roˇw3.1;
                   struct Row2.2;
                   struct Row2.3;ˇ

                   struct Row4;
                   struct ˇRow5.1;
                   struct Row5.2;
                   struct «Rowˇ»5.3;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row9.1;
                   struct «Rowˇ»9.2;
                   struct «ˇRow»9.3;
                   struct Row8;
                   struct Row9;
                   «ˇ// something on bottom»
                   struct Row10;"#},
        vec![
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
            DiffHunkStatusKind::Added,
        ],
        indoc! {r#"struct Row;
                   ˇstruct Row1;
                   struct Row2;
                   ˇ
                   struct Row4;
                   ˇstruct Row5;
                   struct Row6;
                   ˇ
                   ˇstruct Row8;
                   struct Row9;
                   ˇstruct Row10;"#},
        base_text,
        &mut cx,
    );
}

#[gpui::test]
async fn test_modification_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"
        struct Row;
        struct Row1;
        struct Row2;

        struct Row4;
        struct Row5;
        struct Row6;

        struct Row8;
        struct Row9;
        struct Row10;"#};

    // Modification hunks behave the same as the addition ones.
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   ˇ
                   struct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Modified, DiffHunkStatusKind::Modified],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   ˇ
                   struct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Modified, DiffHunkStatusKind::Modified],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row33;
                   «ˇ
                   struct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row99;
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );

    assert_hunk_revert(
        indoc! {r#"ˇstruct Row1.1;
                   struct Row1;
                   «ˇstr»uct Row22;

                   struct ˇRow44;
                   struct Row5;
                   struct «Rˇ»ow66;ˇ

                   «struˇ»ct Row88;
                   struct Row9;
                   struct Row1011;ˇ"#},
        vec![
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
            DiffHunkStatusKind::Modified,
        ],
        indoc! {r#"struct Row;
                   ˇstruct Row1;
                   struct Row2;
                   ˇ
                   struct Row4;
                   ˇstruct Row5;
                   struct Row6;
                   ˇ
                   struct Row8;
                   ˇstruct Row9;
                   struct Row10;ˇ"#},
        base_text,
        &mut cx,
    );
}

#[gpui::test]
async fn test_deleting_over_diff_hunk(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"
        one

        two
        three
        "#};

    cx.set_head_text(base_text);
    cx.set_state("\nˇ\n");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _window, cx| {
        editor.expand_selected_diff_hunks(cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.backspace(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    cx.assert_state_with_diff(
        indoc! {r#"

        - two
        - threeˇ
        +
        "#}
        .to_string(),
    );
}

#[gpui::test]
async fn test_deletion_reverts(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    let base_text = indoc! {r#"struct Row;
struct Row1;
struct Row2;

struct Row4;
struct Row5;
struct Row6;

struct Row8;
struct Row9;
struct Row10;"#};

    // Deletion hunks trigger with carets on adjacent rows, so carets and selections have to stay farther to avoid the revert
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row2;

                   ˇstruct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row8;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Deleted, DiffHunkStatusKind::Deleted],
        indoc! {r#"struct Row;
                   struct Row2;

                   ˇstruct Row4;
                   struct Row5;
                   struct Row6;
                   ˇ
                   struct Row8;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row2;

                   «ˇstruct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row8;
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Deleted, DiffHunkStatusKind::Deleted],
        indoc! {r#"struct Row;
                   struct Row2;

                   «ˇstruct Row4;
                   struct» Row5;
                   «struct Row6;
                   ˇ»
                   struct Row8;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );

    // Deletion hunks are ephemeral, so it's impossible to place the caret into them — Mav triggers reverts for lines, adjacent to carets and selections.
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   ˇstruct Row2;

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;ˇ
                   struct Row10;"#},
        vec![DiffHunkStatusKind::Deleted, DiffHunkStatusKind::Deleted],
        indoc! {r#"struct Row;
                   struct Row1;
                   ˇstruct Row2;

                   struct Row4;
                   struct Row5;
                   struct Row6;

                   struct Row8;ˇ
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
    assert_hunk_revert(
        indoc! {r#"struct Row;
                   struct Row2«ˇ;
                   struct Row4;
                   struct» Row5;
                   «struct Row6;

                   struct Row8;ˇ»
                   struct Row10;"#},
        vec![
            DiffHunkStatusKind::Deleted,
            DiffHunkStatusKind::Deleted,
            DiffHunkStatusKind::Deleted,
        ],
        indoc! {r#"struct Row;
                   struct Row1;
                   struct Row2«ˇ;

                   struct Row4;
                   struct» Row5;
                   «struct Row6;

                   struct Row8;ˇ»
                   struct Row9;
                   struct Row10;"#},
        base_text,
        &mut cx,
    );
}

use super::*;

#[gpui::test]
async fn test_indent_guide_single_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..3, vec![indent_guide(buffer_id, 1, 1, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_simple_block(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
            let b = 2;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..4, vec![indent_guide(buffer_id, 1, 2, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_nested(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..8,
        vec![
            indent_guide(buffer_id, 1, 6, 0),
            indent_guide(buffer_id, 3, 3, 1),
            indent_guide(buffer_id, 5, 5, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_tab(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;
                let b = 2;
            let c = 3;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..5,
        vec![
            indent_guide(buffer_id, 1, 3, 0),
            indent_guide(buffer_id, 2, 2, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_continues_on_empty_line(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(0..5, vec![indent_guide(buffer_id, 1, 3, 0)], None, &mut cx);
}

#[gpui::test]
async fn test_indent_guide_complex(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;

            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..11,
        vec![
            indent_guide(buffer_id, 1, 9, 0),
            indent_guide(buffer_id, 6, 6, 1),
            indent_guide(buffer_id, 8, 8, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_starts_off_screen(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;

            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        1..11,
        vec![
            indent_guide(buffer_id, 1, 9, 0),
            indent_guide(buffer_id, 6, 6, 1),
            indent_guide(buffer_id, 8, 8, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_ends_off_screen(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            let a = 1;

            let c = 3;

            if a == 3 {
                let b = 2;
            } else {
                let c = 3;
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        1..10,
        vec![
            indent_guide(buffer_id, 1, 9, 0),
            indent_guide(buffer_id, 6, 6, 1),
            indent_guide(buffer_id, 8, 8, 1),
        ],
        None,
        &mut cx,
    );
}

#[gpui::test]
async fn test_indent_guide_with_folds(cx: &mut TestAppContext) {
    let (buffer_id, mut cx) = setup_indent_guides_editor(
        &"
        fn main() {
            if a {
                b(
                    c,
                    d,
                )
            } else {
                e(
                    f
                )
            }
        }"
        .unindent(),
        cx,
    )
    .await;

    assert_indent_guides(
        0..11,
        vec![
            indent_guide(buffer_id, 1, 10, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 7, 9, 1),
            indent_guide(buffer_id, 3, 4, 2),
            indent_guide(buffer_id, 8, 8, 2),
        ],
        None,
        &mut cx,
    );

    cx.update_editor(|editor, window, cx| {
        editor.fold_at(MultiBufferRow(2), window, cx);
        assert_eq!(
            editor.display_text(cx),
            "
            fn main() {
                if a {
                    b(⋯)
                } else {
                    e(
                        f
                    )
                }
            }"
            .unindent()
        );
    });

    assert_indent_guides(
        0..11,
        vec![
            indent_guide(buffer_id, 1, 10, 0),
            indent_guide(buffer_id, 2, 5, 1),
            indent_guide(buffer_id, 7, 9, 1),
            indent_guide(buffer_id, 8, 8, 2),
        ],
        None,
        &mut cx,
    );
}

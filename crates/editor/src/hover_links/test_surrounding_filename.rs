use super::*;

#[gpui::test]
async fn test_surrounding_filename(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    let test_cases = [
        ("file ˇ name", None),
        ("ˇfile name", Some("file")),
        ("file ˇname", Some("name")),
        ("fiˇle name", Some("file")),
        ("filenˇame", Some("filename")),
        // Absolute path
        ("foobar ˇ/home/user/f.txt", Some("/home/user/f.txt")),
        ("foobar /home/useˇr/f.txt", Some("/home/user/f.txt")),
        // Windows
        ("C:\\Useˇrs\\user\\f.txt", Some("C:\\Users\\user\\f.txt")),
        // Whitespace
        ("ˇfile\\ -\\ name.txt", Some("file - name.txt")),
        ("file\\ -\\ naˇme.txt", Some("file - name.txt")),
        // Tilde
        ("ˇ~/file.txt", Some("~/file.txt")),
        ("~/fiˇle.txt", Some("~/file.txt")),
        // Double quotes
        ("\"fˇile.txt\"", Some("file.txt")),
        ("ˇ\"file.txt\"", Some("file.txt")),
        ("ˇ\"fi\\ le.txt\"", Some("fi le.txt")),
        // Single quotes
        ("'fˇile.txt'", Some("file.txt")),
        ("ˇ'file.txt'", Some("file.txt")),
        ("ˇ'fi\\ le.txt'", Some("fi le.txt")),
        // Quoted multibyte characters
        (" ˇ\"常\"", Some("常")),
        (" \"ˇ常\"", Some("常")),
        ("ˇ\"常\"", Some("常")),
        // Backticks (surrounding_filename returns the full token including backticks)
        ("`fiˇle.txt`", Some("`file.txt`")),
        ("open `fiˇle.txt` please", Some("`file.txt`")),
        // Parentheses (surrounding_filename returns the full token including parens)
        ("(fiˇle.txt)", Some("(file.txt)")),
        ("open (fiˇle.txt) please", Some("(file.txt)")),
    ];

    for (input, expected) in test_cases {
        cx.set_state(input);

        let (position, snapshot) = cx.editor(|editor, _, cx| {
            let positions = editor
                .selections
                .newest_anchor()
                .head()
                .expect_text_anchor();
            let snapshot = editor
                .buffer()
                .clone()
                .read(cx)
                .as_singleton()
                .unwrap()
                .read(cx)
                .snapshot();
            (positions, snapshot)
        });

        let result = surrounding_filename(&snapshot, position);

        if let Some(expected) = expected {
            assert!(result.is_some(), "Failed to find file path: {}", input);
            let (_, path) = result.unwrap();
            assert_eq!(&path, expected, "Incorrect file path for input: {}", input);
        } else {
            assert!(
                result.is_none(),
                "Expected no result, but got one: {:?}",
                result
            );
        }
    }
}

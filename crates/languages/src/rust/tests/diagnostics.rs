use super::*;

#[gpui::test]
async fn test_process_rust_diagnostics() {
    let mut params = lsp::PublishDiagnosticsParams {
        uri: lsp::Uri::from_file_path(path!("/a")).unwrap(),
        version: None,
        diagnostics: vec![
            // no newlines
            lsp::Diagnostic {
                message: "use of moved value `a`".to_string(),
                ..Default::default()
            },
            // newline at the end of a code span
            lsp::Diagnostic {
                message: "consider importing this struct: `use b::c;\n`".to_string(),
                ..Default::default()
            },
            // code span starting right after a newline
            lsp::Diagnostic {
                message: "cannot borrow `self.d` as mutable\n`self` is a `&` reference".to_string(),
                ..Default::default()
            },
        ],
    };
    RustLspAdapter.process_diagnostics(&mut params, LanguageServerId(0));

    assert_eq!(params.diagnostics[0].message, "use of moved value `a`");

    // remove trailing newline from code span
    assert_eq!(
        params.diagnostics[1].message,
        "consider importing this struct: `use b::c;`"
    );

    // do not remove newline before the start of code span
    assert_eq!(
        params.diagnostics[2].message,
        "cannot borrow `self.d` as mutable\n`self` is a `&` reference"
    );
}

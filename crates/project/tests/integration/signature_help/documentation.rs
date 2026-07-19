use super::*;

#[gpui::test]
fn test_parameter_documentation(cx: &mut TestAppContext) {
    let signature_help = lsp::SignatureHelp {
        signatures: vec![lsp::SignatureInformation {
            label: "fn test(foo: u8, bar: &str)".to_string(),
            documentation: Some(Documentation::String(
                "This is a test documentation".to_string(),
            )),
            parameters: Some(vec![
                lsp::ParameterInformation {
                    label: lsp::ParameterLabel::Simple("foo: u8".to_string()),
                    documentation: Some(Documentation::String("The foo parameter".to_string())),
                },
                lsp::ParameterInformation {
                    label: lsp::ParameterLabel::Simple("bar: &str".to_string()),
                    documentation: Some(Documentation::String("The bar parameter".to_string())),
                },
            ]),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    };
    let maybe_signature_help = cx.update(|cx| SignatureHelp::new(signature_help, None, None, cx));
    assert!(maybe_signature_help.is_some());

    let signature_help = maybe_signature_help.unwrap();
    let signature = &signature_help.signatures[signature_help.active_signature];

    assert_eq!(signature.parameters.len(), 2);
    assert_eq!(
        signature.parameters[0]
            .documentation
            .as_ref()
            .unwrap()
            .update(cx, |documentation, _| documentation.source().to_string()),
        "The foo parameter",
    );
    assert_eq!(
        signature.parameters[1]
            .documentation
            .as_ref()
            .unwrap()
            .update(cx, |documentation, _| documentation.source().to_string()),
        "The bar parameter",
    );

    assert_eq!(signature.active_parameter, Some(0));
}

#[gpui::test]
fn test_create_signature_help_implements_utf16_spec(cx: &mut TestAppContext) {
    let signature_help = lsp::SignatureHelp {
        signatures: vec![lsp::SignatureInformation {
            label: "fn test(🦀: u8, 🦀: &str)".to_string(),
            documentation: None,
            parameters: Some(vec![
                lsp::ParameterInformation {
                    label: lsp::ParameterLabel::LabelOffsets([8, 10]),
                    documentation: None,
                },
                lsp::ParameterInformation {
                    label: lsp::ParameterLabel::LabelOffsets([16, 18]),
                    documentation: None,
                },
            ]),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    };
    let signature_help = cx.update(|cx| SignatureHelp::new(signature_help, None, None, cx));
    assert!(signature_help.is_some());

    let markdown = signature_help.unwrap();
    let signature = markdown.signatures[markdown.active_signature].clone();
    let markdown = (signature.label, signature.highlights);
    assert_eq!(
        markdown,
        (
            SharedString::new_static("fn test(🦀: u8, 🦀: &str)"),
            vec![(8..12, current_parameter())]
        )
    );
}

use super::*;

#[test]
fn text_resource_markdown_uses_mime_type_for_code_blocks() {
    let shell = acp::TextResourceContents::new("echo 'hello from exec test'", "tool://preview")
        .mime_type("text/x-shellscript".to_string());
    assert_eq!(
        ContentBlock::text_resource_markdown(&shell),
        "```sh\necho 'hello from exec test'\n```"
    );

    let markdown = acp::TextResourceContents::new("**approval** requested", "tool://preview")
        .mime_type("text/markdown".to_string());
    assert_eq!(
        ContentBlock::text_resource_markdown(&markdown),
        "**approval** requested"
    );

    let plain = acp::TextResourceContents::new("plain preview", "tool://preview")
        .mime_type("text/plain".to_string());
    assert_eq!(
        ContentBlock::text_resource_markdown(&plain),
        "```\nplain preview\n```"
    );

    let cpp = acp::TextResourceContents::new("int main() {}", "tool://preview")
        .mime_type("text/x-c++; charset=utf-8".to_string());
    assert_eq!(
        ContentBlock::text_resource_markdown(&cpp),
        "```cpp\nint main() {}\n```"
    );

    let untyped = acp::TextResourceContents::new("# plain preview", "tool://preview");
    assert_eq!(
        ContentBlock::text_resource_markdown(&untyped),
        "```\n# plain preview\n```"
    );
}

#[gpui::test]
async fn test_tool_call_content_preserves_embedded_text_resource(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        let content = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
            acp::EmbeddedResourceResource::TextResourceContents(
                acp::TextResourceContents::new("echo 'hello from exec test'", "tool://preview")
                    .mime_type("text/x-shellscript".to_string()),
            ),
        ));

        let block = ContentBlock::new_tool_call_content(
            content,
            &language_registry,
            PathStyle::local(),
            cx,
        );

        let ContentBlock::EmbeddedResource { resource, markdown } = &block else {
            panic!("expected embedded resource block, got {block:?}");
        };
        match &resource.resource {
            acp::EmbeddedResourceResource::TextResourceContents(text) => {
                assert_eq!(text.text, "echo 'hello from exec test'");
                assert_eq!(text.uri, "tool://preview");
                assert_eq!(text.mime_type.as_deref(), Some("text/x-shellscript"));
            }
            other => panic!("expected text resource contents, got {other:?}"),
        }

        let markdown = markdown
            .as_ref()
            .expect("text resources should have renderable markdown")
            .read(cx)
            .source()
            .to_string();
        assert_eq!(markdown, "```sh\necho 'hello from exec test'\n```");
        assert_eq!(
            block.to_markdown(cx),
            "```sh\necho 'hello from exec test'\n```"
        );
        assert_eq!(block.text_content(cx), Some("echo 'hello from exec test'"));

        let untyped = ContentBlock::new_tool_call_content(
            acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                acp::EmbeddedResourceResource::TextResourceContents(
                    acp::TextResourceContents::new("# plain preview", "tool://preview"),
                ),
            )),
            &language_registry,
            PathStyle::local(),
            cx,
        );
        assert_eq!(untyped.to_markdown(cx), "```\n# plain preview\n```");
        assert_eq!(untyped.text_content(cx), Some("# plain preview"));
    });
}

#[gpui::test]
async fn test_tool_call_content_renders_embedded_image_blob_resource(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    cx.update(|cx| {
        let language_registry =
            Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        let image_blob = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
            acp::EmbeddedResourceResource::BlobResourceContents(
                acp::BlobResourceContents::new(
                    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
                    "tool://preview.png",
                )
                .mime_type("image/png".to_string()),
            ),
        ));

        let block = ContentBlock::new_tool_call_content(
            image_blob,
            &language_registry,
            PathStyle::local(),
            cx,
        );

        let ContentBlock::Image { image, dimensions } = &block else {
            panic!("expected image block, got {block:?}");
        };
        assert_eq!(image.format(), gpui::ImageFormat::Png);
        assert_eq!(
            dimensions.as_ref().map(|size| (size.width, size.height)),
            Some((1, 1))
        );
        assert_eq!(block.to_markdown(cx), "`Image`");
        assert_eq!(block.text_content(cx), None);
    });
}

#[gpui::test]
async fn test_tool_call_content_falls_back_for_non_image_blob_resource(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    cx.update(|cx| {
        let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
        let archive_blob = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
            acp::EmbeddedResourceResource::BlobResourceContents(
                acp::BlobResourceContents::new("not an image", "tool://archive.bin")
                    .mime_type("application/octet-stream".to_string()),
            ),
        ));

        let block = ContentBlock::new_tool_call_content(
            archive_blob,
            &language_registry,
            PathStyle::local(),
            cx,
        );

        let ContentBlock::EmbeddedResource { resource, markdown } = &block else {
            panic!("expected embedded resource block, got {block:?}");
        };
        assert!(markdown.is_none());
        match &resource.resource {
            acp::EmbeddedResourceResource::BlobResourceContents(blob) => {
                assert_eq!(blob.uri, "tool://archive.bin");
                assert_eq!(blob.mime_type.as_deref(), Some("application/octet-stream"));
            }
            other => panic!("expected blob resource contents, got {other:?}"),
        }
        assert_eq!(block.to_markdown(cx), "tool://archive.bin");
        assert_eq!(block.text_content(cx), None);

        let invalid_image_blob = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
            acp::EmbeddedResourceResource::BlobResourceContents(
                acp::BlobResourceContents::new("not-base64", "tool://preview.png")
                    .mime_type("image/png".to_string()),
            ),
        ));
        let invalid = ContentBlock::new_tool_call_content(
            invalid_image_blob,
            &language_registry,
            PathStyle::local(),
            cx,
        );
        let ContentBlock::EmbeddedResource { resource, markdown } = &invalid else {
            panic!("expected embedded resource block, got {invalid:?}");
        };
        assert!(markdown.is_none());
        assert_eq!(
            ContentBlock::embedded_resource_label(resource),
            "tool://preview.png"
        );
        assert_eq!(invalid.to_markdown(cx), "tool://preview.png");
    });
}

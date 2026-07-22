use super::*;

#[gpui::test]
fn test_autoindent_with_injected_languages(cx: &mut App) {
    init_settings(cx, |settings| {
        settings.languages.0.extend([
            (
                "HTML".into(),
                LanguageSettingsContent {
                    tab_size: Some(2.try_into().unwrap()),
                    ..Default::default()
                },
            ),
            (
                "JavaScript".into(),
                LanguageSettingsContent {
                    tab_size: Some(8.try_into().unwrap()),
                    ..Default::default()
                },
            ),
        ])
    });

    let html_language = Arc::new(html_lang());

    let javascript_language = Arc::new(javascript_lang());

    let language_registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    language_registry.add(html_language.clone());
    language_registry.add(javascript_language);

    cx.new(|cx| {
        let (text, ranges) = marked_text_ranges(
            &"
                <div>ˇ
                </div>
                <script>
                    init({ˇ
                    })
                </script>
                <span>ˇ
                </span>
            "
            .unindent(),
            false,
        );

        let mut buffer = Buffer::local(text, cx);
        buffer.set_language_registry(language_registry);
        buffer.set_language(Some(html_language), cx);
        buffer.edit(
            ranges.into_iter().map(|range| (range, "\na")),
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "
                <div>
                  a
                </div>
                <script>
                    init({
                            a
                    })
                </script>
                <span>
                  a
                </span>
            "
            .unindent()
        );
        buffer
    });
}

#[gpui::test]
fn test_autoindent_query_with_outdent_captures(cx: &mut App) {
    init_settings(cx, |settings| {
        settings.defaults.tab_size = Some(2.try_into().unwrap());
    });

    cx.new(|cx| {
        let mut buffer = Buffer::local("", cx).with_language(Arc::new(ruby_lang()), cx);

        let text = r#"
            class C
            def a(b, c)
            puts b
            puts c
            rescue
            puts "errored"
            exit 1
            end
            end
        "#
        .unindent();

        buffer.edit([(0..0, text)], Some(AutoindentMode::EachLine), cx);

        assert_eq!(
            buffer.text(),
            r#"
                class C
                  def a(b, c)
                    puts b
                    puts c
                  rescue
                    puts "errored"
                    exit 1
                  end
                end
            "#
            .unindent()
        );

        buffer
    });
}

#[gpui::test]
async fn test_async_autoindents_preserve_preview(cx: &mut TestAppContext) {
    cx.update(|cx| init_settings(cx, |_| {}));

    // First we insert some newlines to request an auto-indent (asynchronously).
    // Then we request that a preview tab be preserved for the new version, even though it's edited.
    let buffer = cx.new(|cx| {
        let text = "fn a() {}";
        let mut buffer = Buffer::local(text, cx).with_language(rust_lang(), cx);

        // This causes autoindent to be async.
        buffer.set_sync_parse_timeout(None);

        buffer.edit([(8..8, "\n\n")], Some(AutoindentMode::EachLine), cx);
        buffer.refresh_preview();

        // Synchronously, we haven't auto-indented and we're still preserving the preview.
        assert_eq!(buffer.text(), "fn a() {\n\n}");
        assert!(buffer.preserve_preview());
        buffer
    });

    // Now let the autoindent finish
    cx.executor().run_until_parked();

    // The auto-indent applied, but didn't dismiss our preview
    buffer.update(cx, |buffer, cx| {
        assert_eq!(buffer.text(), "fn a() {\n    \n}");
        assert!(buffer.preserve_preview());

        // Edit inserting another line. It will autoindent async.
        // Then refresh the preview version.
        buffer.edit(
            [(Point::new(1, 4)..Point::new(1, 4), "\n")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        buffer.refresh_preview();
        assert_eq!(buffer.text(), "fn a() {\n    \n\n}");
        assert!(buffer.preserve_preview());

        // Then perform another edit, this time without refreshing the preview version.
        buffer.edit([(Point::new(1, 4)..Point::new(1, 4), "x")], None, cx);
        // This causes the preview to not be preserved.
        assert!(!buffer.preserve_preview());
    });

    // Let the async autoindent from the first edit finish.
    cx.executor().run_until_parked();

    // The autoindent applies, but it shouldn't restore the preview status because we had an edit in the meantime.
    buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.text(), "fn a() {\n    x\n    \n}");
        assert!(!buffer.preserve_preview());
    });
}

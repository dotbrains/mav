use super::*;

#[gpui::test]
async fn test_python_autoindent(cx: &mut TestAppContext) {
    cx.executor().set_block_on_ticks(usize::MAX..=usize::MAX);
    let language = crate::language("python", tree_sitter_python::LANGUAGE.into());
    cx.update(|cx| {
        let test_settings = SettingsStore::test(cx);
        cx.set_global(test_settings);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |s| {
                s.project.all_languages.defaults.tab_size = NonZeroU32::new(2);
            });
        });
    });

    cx.new(|cx| {
        let mut buffer = Buffer::local("", cx).with_language(language, cx);
        let append = |buffer: &mut Buffer, text: &str, cx: &mut Context<Buffer>| {
            let ix = buffer.len();
            buffer.edit([(ix..ix, text)], Some(AutoindentMode::EachLine), cx);
        };

        // indent after "def():"
        append(&mut buffer, "def a():\n", cx);
        assert_eq!(buffer.text(), "def a():\n  ");

        // preserve indent after blank line
        append(&mut buffer, "\n  ", cx);
        assert_eq!(buffer.text(), "def a():\n  \n  ");

        // indent after "if"
        append(&mut buffer, "if a:\n  ", cx);
        assert_eq!(buffer.text(), "def a():\n  \n  if a:\n    ");

        // preserve indent after statement
        append(&mut buffer, "b()\n", cx);
        assert_eq!(buffer.text(), "def a():\n  \n  if a:\n    b()\n    ");

        // preserve indent after statement
        append(&mut buffer, "else", cx);
        assert_eq!(buffer.text(), "def a():\n  \n  if a:\n    b()\n    else");

        // dedent "else""
        append(&mut buffer, ":", cx);
        assert_eq!(buffer.text(), "def a():\n  \n  if a:\n    b()\n  else:");

        // indent lines after else
        append(&mut buffer, "\n", cx);
        assert_eq!(
            buffer.text(),
            "def a():\n  \n  if a:\n    b()\n  else:\n    "
        );

        // indent after an open paren. the closing paren is not indented
        // because there is another token before it on the same line.
        append(&mut buffer, "foo(\n1)", cx);
        assert_eq!(
            buffer.text(),
            "def a():\n  \n  if a:\n    b()\n  else:\n    foo(\n      1)"
        );

        // dedent the closing paren if it is shifted to the beginning of the line
        let argument_ix = buffer.text().find('1').unwrap();
        buffer.edit(
            [(argument_ix..argument_ix + 1, "")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "def a():\n  \n  if a:\n    b()\n  else:\n    foo(\n    )"
        );

        // preserve indent after the close paren
        append(&mut buffer, "\n", cx);
        assert_eq!(
            buffer.text(),
            "def a():\n  \n  if a:\n    b()\n  else:\n    foo(\n    )\n    "
        );

        // manually outdent the last line
        let end_whitespace_ix = buffer.len() - 4;
        buffer.edit(
            [(end_whitespace_ix..buffer.len(), "")],
            Some(AutoindentMode::EachLine),
            cx,
        );
        assert_eq!(
            buffer.text(),
            "def a():\n  \n  if a:\n    b()\n  else:\n    foo(\n    )\n"
        );

        // preserve the newly reduced indentation on the next newline
        append(&mut buffer, "\n", cx);
        assert_eq!(
            buffer.text(),
            "def a():\n  \n  if a:\n    b()\n  else:\n    foo(\n    )\n\n"
        );

        // reset to a for loop statement
        let statement = "for i in range(10):\n  print(i)\n";
        buffer.edit([(0..buffer.len(), statement)], None, cx);

        // insert single line comment after each line
        let eol_ixs = statement
            .char_indices()
            .filter_map(|(ix, c)| if c == '\n' { Some(ix) } else { None })
            .collect::<Vec<usize>>();
        let editions = eol_ixs
            .iter()
            .enumerate()
            .map(|(i, &eol_ix)| (eol_ix..eol_ix, format!(" # comment {}", i + 1)))
            .collect::<Vec<(std::ops::Range<usize>, String)>>();
        buffer.edit(editions, Some(AutoindentMode::EachLine), cx);
        assert_eq!(
            buffer.text(),
            "for i in range(10): # comment 1\n  print(i) # comment 2\n"
        );

        // reset to a simple if statement
        buffer.edit([(0..buffer.len(), "if a:\n  b(\n  )")], None, cx);

        // dedent "else" on the line after a closing paren
        append(&mut buffer, "\n  else:\n", cx);
        assert_eq!(buffer.text(), "if a:\n  b(\n  )\nelse:\n  ");

        buffer
    });
}

#[test]
fn test_python_module_name_from_relative_path() {
    assert_eq!(
        python_module_name_from_relative_path("foo/bar.py"),
        Some("foo.bar".to_string())
    );
    assert_eq!(
        python_module_name_from_relative_path("foo/bar"),
        Some("foo.bar".to_string())
    );
    if cfg!(windows) {
        assert_eq!(
            python_module_name_from_relative_path("foo\\bar.py"),
            Some("foo.bar".to_string())
        );
        assert_eq!(
            python_module_name_from_relative_path("foo\\bar"),
            Some("foo.bar".to_string())
        );
    } else {
        assert_eq!(
            python_module_name_from_relative_path("foo\\bar.py"),
            Some("foo\\bar".to_string())
        );
        assert_eq!(
            python_module_name_from_relative_path("foo\\bar"),
            Some("foo\\bar".to_string())
        );
    }
}

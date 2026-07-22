use super::*;

#[gpui::test]
fn test_combined_injections_simple(cx: &mut App) {
    let (buffer, syntax_map) = test_edit_sequence(
        "ERB",
        &[
            "
                <body>
                    <% if @one %>
                        <div class=one>
                    <% else %>
                        <div class=two>
                    <% end %>
                    </div>
                </body>
            ",
            "
                <body>
                    <% if @one %>
                        <div class=one>
                    ˇ else ˇ
                        <div class=two>
                    <% end %>
                    </div>
                </body>
            ",
            "
                <body>
                    <% if @one «;» end %>
                    </div>
                </body>
            ",
        ],
        cx,
    );

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["tag", "ivar"],
        "
            <«body»>
                <% if «@one» ; end %>
                </«div»>
            </«body»>
        ",
    );
}

#[gpui::test]
fn test_combined_injections_empty_ranges(cx: &mut App) {
    test_edit_sequence(
        "ERB",
        &[
            "
                <% if @one %>
                <% else %>
                <% end %>
            ",
            "
                <% if @one %>
                ˇ<% end %>
            ",
        ],
        cx,
    );
}

#[gpui::test]
fn test_combined_injections_edit_edges_of_ranges(cx: &mut App) {
    let (buffer, syntax_map) = test_edit_sequence(
        "ERB",
        &[
            "
                <%= one @two %>
                <%= three @four %>
            ",
            "
                <%= one @two %ˇ
                <%= three @four %>
            ",
            "
                <%= one @two %«>»
                <%= three @four %>
            ",
        ],
        cx,
    );

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["tag", "ivar"],
        "
            <%= one «@two» %>
            <%= three «@four» %>
        ",
    );
}

#[gpui::test]
fn test_combined_injections_splitting_some_injections(cx: &mut App) {
    let (_buffer, _syntax_map) = test_edit_sequence(
        "ERB",
        &[
            r#"
                <%A if b(:c) %>
                d
                <% end %>
                eee
                <% f %>
            "#,
            r#"
                <%« AAAAAAA %>
                hhhhhhh
                <%=» if b(:c) %>
                d
                <% end %>
                eee
                <% f %>
            "#,
        ],
        cx,
    );
}

#[gpui::test]
fn test_combined_injections_editing_after_last_injection(cx: &mut App) {
    test_edit_sequence(
        "ERB",
        &[
            r#"
                <% foo %>
                <div></div>
                <% bar %>
            "#,
            r#"
                <% foo %>
                <div></div>
                <% bar %>«
                more text»
            "#,
        ],
        cx,
    );
}

#[gpui::test]
fn test_combined_injections_inside_injections(cx: &mut App) {
    let (buffer, syntax_map) = test_edit_sequence(
        "Markdown",
        &[
            r#"
                here is
                some
                ERB code:

                ```erb
                <ul>
                <% people.each do |person| %>
                    <li><%= person.name %></li>
                    <li><%= person.age %></li>
                <% end %>
                </ul>
                ```
            "#,
            r#"
                here is
                some
                ERB code:

                ```erb
                <ul>
                <% people«2».each do |person| %>
                    <li><%= person.name %></li>
                    <li><%= person.age %></li>
                <% end %>
                </ul>
                ```
            "#,
            // Inserting a comment character inside one code directive
            // does not cause the other code directive to become a comment,
            // because newlines are included in between each injection range.
            r#"
                here is
                some
                ERB code:

                ```erb
                <ul>
                <% people2.each do |person| %>
                    <li><%= «# »person.name %></li>
                    <li><%= person.age %></li>
                <% end %>
                </ul>
                ```
            "#,
        ],
        cx,
    );

    // Check that the code directive below the ruby comment is
    // not parsed as a comment.
    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["method"],
        "
            here is
            some
            ERB code:

            ```erb
            <ul>
            <% people2.«each» do |person| %>
                <li><%= # person.name %></li>
                <li><%= person.«age» %></li>
            <% end %>
            </ul>
            ```
        ",
    );
}

#[gpui::test]
fn test_empty_combined_injections_inside_injections(cx: &mut App) {
    let (buffer, syntax_map) = test_edit_sequence(
        "Markdown",
        &[r#"
            ```erb
            hello
            ```

            goodbye
        "#],
        cx,
    );

    assert_layers_for_range(
        &syntax_map,
        &buffer,
        Point::new(0, 0)..Point::new(5, 0),
        &[
            // Markdown document
            "(document (section (fenced_code_block (fenced_code_block_delimiter) (info_string (language)) (block_continuation) (code_fence_content (block_continuation)) (fenced_code_block_delimiter)) (paragraph (inline))))",
            // ERB template in the code block
            "(template...",
            // Markdown inline content
            "(inline)",
            // The ruby syntax tree should be empty, since there are
            // no interpolations in the ERB template.
            "(program)",
            // HTML within the ERB
            "(document (text))",
        ],
    );
}

#[gpui::test]
fn test_comment_triggered_injection_toggle(cx: &mut App) {
    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));

    let python = Arc::new(python_lang());
    let comment = Arc::new(comment_lang());
    registry.add(python.clone());
    registry.add(comment);
    // Note: SQL is an extension language (not built-in as of v0.222.0), so we can use
    // contains_unknown_injections() to detect when the injection is triggered.
    // We register a mock "comment" language because Python injects all comments as
    // language "comment", and we only want SQL to trigger unknown injections.

    // Start with Python code with incomplete #sq comment (not enough to trigger injection)
    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        "#sq\ncmd = \"SELECT col1, col2 FROM tbl\"".to_string(),
    );

    let mut syntax_map = SyntaxMap::new(&buffer);
    syntax_map.set_language_registry(registry);
    syntax_map.reparse(python.clone(), &buffer);

    // Should have no unknown injections (#sq doesn't match the injection pattern)
    assert!(
        !syntax_map.contains_unknown_injections(),
        "Expected no unknown injections with incomplete #sq comment"
    );

    // Complete the comment by adding 'l' to make #sql
    let sq_end = buffer.as_rope().to_string().find('\n').unwrap();
    buffer.edit([(sq_end..sq_end, "l")]);
    syntax_map.interpolate(&buffer);
    syntax_map.reparse(python.clone(), &buffer);

    // Should now have unknown injections (SQL injection triggered but SQL not registered)
    assert!(
        syntax_map.contains_unknown_injections(),
        "Expected unknown injections after completing #sql comment"
    );

    // Remove the 'l' to go back to #sq
    let l_position = buffer.as_rope().to_string().find("l\n").unwrap();
    buffer.edit([(l_position..l_position + 1, "")]);
    syntax_map.interpolate(&buffer);
    syntax_map.reparse(python, &buffer);

    // Should have no unknown injections again - SQL injection should be invalidated
    assert!(
        !syntax_map.contains_unknown_injections(),
        "Expected no unknown injections after removing 'l' from #sql comment"
    );
}

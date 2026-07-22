use super::*;

#[gpui::test]
fn test_syntax_map_languages_loading_with_erb(cx: &mut App) {
    let text = r#"
        <body>
            <% if @one %>
                <div class=one>
            <% else %>
                <div class=two>
            <% end %>
            </div>
        </body>
    "#
    .unindent();

    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text);

    let mut syntax_map = SyntaxMap::new(&buffer);
    syntax_map.set_language_registry(registry.clone());

    let language = Arc::new(erb_lang());

    log::info!("parsing");
    registry.add(language.clone());
    syntax_map.reparse(language.clone(), &buffer);

    log::info!("loading html");
    registry.add(Arc::new(html_lang()));
    syntax_map.reparse(language.clone(), &buffer);

    log::info!("loading ruby");
    registry.add(Arc::new(ruby_lang()));
    syntax_map.reparse(language.clone(), &buffer);

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["tag", "ivar"],
        "
            <«body»>
                <% if «@one» %>
                    <«div» class=one>
                <% else %>
                    <«div» class=two>
                <% end %>
                </«div»>
            </«body»>
        ",
    );

    let text = r#"
        <body>
            <% if @one«_hundred» %>
                <div class=one>
            <% else %>
                <div class=two>
            <% end %>
            </div>
        </body>
    "#
    .unindent();

    log::info!("editing");
    buffer.edit_via_marked_text(&text);
    syntax_map.interpolate(&buffer);
    syntax_map.reparse(language, &buffer);

    assert_capture_ranges(
        &syntax_map,
        &buffer,
        &["tag", "ivar"],
        "
            <«body»>
                <% if «@one_hundred» %>
                    <«div» class=one>
                <% else %>
                    <«div» class=two>
                <% end %>
                </«div»>
            </«body»>
        ",
    );
}

#[gpui::test(iterations = 50)]
fn test_random_syntax_map_edits_rust_macros(rng: StdRng, cx: &mut App) {
    let text = r#"
        fn test_something() {
            let vec = vec![5, 1, 3, 8];
            assert_eq!(
                vec
                    .into_iter()
                    .map(|i| i * 2)
                    .collect::<Vec<usize>>(),
                vec![
                    5 * 2, 1 * 2, 3 * 2, 8 * 2
                ],
            );
        }
    "#
    .unindent()
    .repeat(2);

    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let language = rust_lang();
    registry.add(language.clone());

    test_random_edits(text, registry, language, rng);
}

#[gpui::test(iterations = 50)]
fn test_random_syntax_map_edits_with_erb(rng: StdRng, cx: &mut App) {
    let text = r#"
        <div id="main">
        <% if one?(:two) %>
            <p class="three" four>
            <%= yield :five %>
            </p>
        <% elsif Six.seven(8) %>
            <p id="three" four>
            <%= yield :five %>
            </p>
        <% else %>
            <span>Ok</span>
        <% end %>
        </div>
    "#
    .unindent()
    .repeat(5);

    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let language = Arc::new(erb_lang());
    registry.add(language.clone());
    registry.add(Arc::new(ruby_lang()));
    registry.add(Arc::new(html_lang()));

    test_random_edits(text, registry, language, rng);
}

#[gpui::test(iterations = 50)]
fn test_random_syntax_map_edits_with_heex(rng: StdRng, cx: &mut App) {
    let text = r#"
        defmodule TheModule do
            def the_method(assigns) do
                ~H"""
                <%= if @empty do %>
                    <div class="h-4"></div>
                <% else %>
                    <div class="max-w-2xl w-full animate-pulse">
                    <div class="flex-1 space-y-4">
                        <div class={[@bg_class, "h-4 rounded-lg w-3/4"]}></div>
                        <div class={[@bg_class, "h-4 rounded-lg"]}></div>
                        <div class={[@bg_class, "h-4 rounded-lg w-5/6"]}></div>
                    </div>
                    </div>
                <% end %>
                """
            end
        end
    "#
    .unindent()
    .repeat(3);

    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    let language = Arc::new(elixir_lang());
    registry.add(language.clone());
    registry.add(Arc::new(heex_lang()));
    registry.add(Arc::new(html_lang()));

    test_random_edits(text, registry, language, rng);
}

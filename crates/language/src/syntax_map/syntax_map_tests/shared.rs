use super::*;

fn test_random_edits(
    text: String,
    registry: Arc<LanguageRegistry>,
    language: Arc<Language>,
    mut rng: StdRng,
) {
    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(10);

    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), text);

    let mut syntax_map = SyntaxMap::new(&buffer);
    syntax_map.set_language_registry(registry.clone());
    syntax_map.reparse(language.clone(), &buffer);

    let mut reference_syntax_map = SyntaxMap::new(&buffer);
    reference_syntax_map.set_language_registry(registry);

    log::info!("initial text:\n{}", buffer.text());

    for _ in 0..operations {
        let prev_buffer = buffer.snapshot().clone();
        let prev_syntax_map = syntax_map.snapshot();

        buffer.randomly_edit(&mut rng, 3);
        log::info!("text:\n{}", buffer.text());

        syntax_map.interpolate(&buffer);
        check_interpolation(&prev_syntax_map, &syntax_map, &prev_buffer, &buffer);

        syntax_map.reparse(language.clone(), &buffer);

        reference_syntax_map.clear(&buffer);
        reference_syntax_map.reparse(language.clone(), &buffer);
    }

    for i in 0..operations {
        let i = operations - i - 1;
        buffer.undo();
        log::info!("undoing operation {}", i);
        log::info!("text:\n{}", buffer.text());

        syntax_map.interpolate(&buffer);
        syntax_map.reparse(language.clone(), &buffer);

        reference_syntax_map.clear(&buffer);
        reference_syntax_map.reparse(language.clone(), &buffer);
        assert_eq!(
            syntax_map.layers(&buffer).len(),
            reference_syntax_map.layers(&buffer).len(),
            "wrong number of layers after undoing edit {i}"
        );
    }

    let layers = syntax_map.layers(&buffer);
    let reference_layers = reference_syntax_map.layers(&buffer);
    for (edited_layer, reference_layer) in layers.into_iter().zip(reference_layers) {
        assert_eq!(
            edited_layer.node().to_sexp(),
            reference_layer.node().to_sexp()
        );
        assert_eq!(edited_layer.node().range(), reference_layer.node().range());
    }
}

fn check_interpolation(
    old_syntax_map: &SyntaxSnapshot,
    new_syntax_map: &SyntaxSnapshot,
    old_buffer: &BufferSnapshot,
    new_buffer: &BufferSnapshot,
) {
    let edits = new_buffer
        .edits_since::<usize>(old_buffer.version())
        .collect::<Vec<_>>();

    for (old_layer, new_layer) in old_syntax_map
        .layers
        .iter()
        .zip(new_syntax_map.layers.iter())
    {
        assert_eq!(old_layer.range, new_layer.range);
        let Some(old_tree) = old_layer.content.tree() else {
            continue;
        };
        let Some(new_tree) = new_layer.content.tree() else {
            continue;
        };
        let old_start_byte = old_layer.range.start.to_offset(old_buffer);
        let new_start_byte = new_layer.range.start.to_offset(new_buffer);
        let old_start_point = old_layer.range.start.to_point(old_buffer).to_ts_point();
        let new_start_point = new_layer.range.start.to_point(new_buffer).to_ts_point();
        let old_node = old_tree.root_node_with_offset(old_start_byte, old_start_point);
        let new_node = new_tree.root_node_with_offset(new_start_byte, new_start_point);
        check_node_edits(
            old_layer.depth,
            &old_layer.range,
            old_node,
            new_node,
            old_buffer,
            new_buffer,
            &edits,
        );
    }

    fn check_node_edits(
        depth: usize,
        range: &Range<Anchor>,
        old_node: Node,
        new_node: Node,
        old_buffer: &BufferSnapshot,
        new_buffer: &BufferSnapshot,
        edits: &[text::Edit<usize>],
    ) {
        assert_eq!(old_node.kind(), new_node.kind());

        let old_range = old_node.byte_range();
        let new_range = new_node.byte_range();

        let is_edited = edits
            .iter()
            .any(|edit| edit.new.start < new_range.end && edit.new.end > new_range.start);
        if is_edited {
            assert!(
                new_node.has_changes(),
                concat!(
                    "failed to mark node as edited.\n",
                    "layer depth: {}, old layer range: {:?}, new layer range: {:?},\n",
                    "node kind: {}, old node range: {:?}, new node range: {:?}",
                ),
                depth,
                range.to_offset(old_buffer),
                range.to_offset(new_buffer),
                new_node.kind(),
                old_range,
                new_range,
            );
        }

        if !new_node.has_changes() {
            assert_eq!(
                old_buffer
                    .text_for_range(old_range.clone())
                    .collect::<String>(),
                new_buffer
                    .text_for_range(new_range.clone())
                    .collect::<String>(),
                concat!(
                    "mismatched text for node\n",
                    "layer depth: {}, old layer range: {:?}, new layer range: {:?},\n",
                    "node kind: {}, old node range:{:?}, new node range:{:?}",
                ),
                depth,
                range.to_offset(old_buffer),
                range.to_offset(new_buffer),
                new_node.kind(),
                old_range,
                new_range,
            );
        }

        for i in 0..new_node.child_count() {
            check_node_edits(
                depth,
                range,
                old_node.child(i as u32).unwrap(),
                new_node.child(i as u32).unwrap(),
                old_buffer,
                new_buffer,
                edits,
            )
        }
    }
}

fn test_edit_sequence(language_name: &str, steps: &[&str], cx: &mut App) -> (Buffer, SyntaxMap) {
    let registry = Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
    registry.add(Arc::new(elixir_lang()));
    registry.add(Arc::new(heex_lang()));
    registry.add(rust_lang());
    registry.add(Arc::new(ruby_lang()));
    registry.add(Arc::new(html_lang()));
    registry.add(Arc::new(erb_lang()));
    registry.add(markdown_lang());
    registry.add(Arc::new(markdown_inline_lang()));

    let language = registry
        .language_for_name(language_name)
        .now_or_never()
        .unwrap()
        .unwrap();
    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), "");

    let mut mutated_syntax_map = SyntaxMap::new(&buffer);
    mutated_syntax_map.set_language_registry(registry.clone());
    mutated_syntax_map.reparse(language.clone(), &buffer);

    for (i, marked_string) in steps.iter().enumerate() {
        let marked_string = marked_string.unindent();
        log::info!("incremental parse {i}: {marked_string:?}");
        buffer.edit_via_marked_text(&marked_string);

        // Reparse the syntax map
        mutated_syntax_map.interpolate(&buffer);
        mutated_syntax_map.reparse(language.clone(), &buffer);

        // Create a second syntax map from scratch
        log::info!("fresh parse {i}: {marked_string:?}");
        let mut reference_syntax_map = SyntaxMap::new(&buffer);
        reference_syntax_map.set_language_registry(registry.clone());
        reference_syntax_map.reparse(language.clone(), &buffer);

        // Compare the mutated syntax map to the new syntax map
        let mutated_layers = mutated_syntax_map.layers(&buffer);
        let reference_layers = reference_syntax_map.layers(&buffer);
        assert_eq!(
            mutated_layers.len(),
            reference_layers.len(),
            "wrong number of layers at step {i}"
        );
        for (edited_layer, reference_layer) in mutated_layers.into_iter().zip(reference_layers) {
            assert_eq!(
                edited_layer.node().to_sexp(),
                reference_layer.node().to_sexp(),
                "different layer at step {i}"
            );
            assert_eq!(
                edited_layer.node().range(),
                reference_layer.node().range(),
                "different layer at step {i}"
            );
        }
    }

    (buffer, mutated_syntax_map)
}

use super::*;

#[track_caller]
fn assert_layers_for_range(
    syntax_map: &SyntaxMap,
    buffer: &BufferSnapshot,
    range: Range<Point>,
    expected_layers: &[&str],
) {
    let layers = syntax_map
        .layers_for_range(range, buffer, true)
        .collect::<Vec<_>>();
    assert_eq!(
        layers.len(),
        expected_layers.len(),
        "wrong number of layers"
    );
    for (i, (layer, expected_s_exp)) in layers.iter().zip(expected_layers.iter()).enumerate() {
        let actual_s_exp = layer.node().to_sexp();
        assert!(
            string_contains_sequence(
                &actual_s_exp,
                &expected_s_exp.split("...").collect::<Vec<_>>()
            ),
            "layer {i}:\n\nexpected: {expected_s_exp}\nactual:   {actual_s_exp}",
        );
    }
}

#[track_caller]
fn assert_capture_ranges(
    syntax_map: &SyntaxMap,
    buffer: &BufferSnapshot,
    highlight_query_capture_names: &[&str],
    marked_string: &str,
) {
    let mut actual_ranges = Vec::<Range<usize>>::new();
    let captures = syntax_map.captures(0..buffer.len(), buffer, |grammar| {
        grammar
            .highlights_config
            .as_ref()
            .map(|config| &config.query)
    });
    let queries = captures
        .grammars()
        .iter()
        .map(|grammar| &grammar.highlights_config.as_ref().unwrap().query)
        .collect::<Vec<_>>();
    for capture in captures {
        let name = &queries[capture.grammar_index].capture_names()[capture.index as usize];
        if highlight_query_capture_names.contains(name) {
            actual_ranges.push(capture.node.byte_range());
        }
    }
    actual_ranges.dedup();

    let (text, expected_ranges) = marked_text_ranges(&marked_string.unindent(), false);
    assert_eq!(text, buffer.text());
    assert_eq!(actual_ranges, expected_ranges);
}

pub fn string_contains_sequence(text: &str, parts: &[&str]) -> bool {
    let mut last_part_end = 0;
    for part in parts {
        if let Some(start_ix) = text[last_part_end..].find(part) {
            last_part_end = start_ix + part.len();
        } else {
            return false;
        }
    }
    true
}

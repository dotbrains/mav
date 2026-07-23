
use super::*;

#[test]
fn test_tokenize_splits_code_identifiers() {
    let tokens =
        tokenize("PrivateNetworkRequestPolicy foo_bar config/reg_default_16M_retrieval.json");

    assert!(tokens.contains(&"privatenetworkrequestpolicy".to_string()));
    assert!(tokens.contains(&"private".to_string()));
    assert!(tokens.contains(&"network".to_string()));
    assert!(tokens.contains(&"request".to_string()));
    assert!(tokens.contains(&"policy".to_string()));
    assert!(tokens.contains(&"foo_bar".to_string()));
    assert!(tokens.contains(&"foo".to_string()));
    assert!(tokens.contains(&"bar".to_string()));
    assert!(tokens.contains(&"reg_default_16m_retrieval".to_string()));
    assert!(tokens.contains(&"retrieval".to_string()));
}

#[test]
fn test_chunk_line_ranges_prefers_empty_line_boundaries_with_overlap() {
    let text = "a\nb\n\nc\nd\ne\nf\n\ng\nh\ni\nj\n";
    let lines = lines(text);
    let ranges = chunk_line_ranges(&lines, 3, 1);

    assert_eq!(ranges[0], 0..3);
    assert!(ranges[1].start < ranges[0].end);
}

#[test]
fn test_bm25_ranks_matching_chunk() {
    let documents = vec![
        Document {
            relative_path: PathBuf::from("src/unrelated.rs"),
            row_range: 0..1,
            term_frequencies: {
                let mut terms = HashMap::new();
                add_term_frequencies(&mut terms, tokenize("fn unrelated"), 1);
                terms
            },
            len: 2,
        },
        Document {
            relative_path: PathBuf::from("src/network.rs"),
            row_range: 0..1,
            term_frequencies: {
                let mut terms = HashMap::new();
                add_term_frequencies(
                    &mut terms,
                    tokenize("fn update_private_network_request_policy"),
                    1,
                );
                terms
            },
            len: 6,
        },
    ];
    let mut document_frequencies = HashMap::new();
    for document in &documents {
        for term in document.term_frequencies.keys() {
            *document_frequencies.entry(term.clone()).or_default() += 1;
        }
    }
    let index = Bm25Index {
        documents,
        document_frequencies,
        average_document_len: 4.0,
        stats: Bm25IndexStats::default(),
    };
    let mut query = HashMap::new();
    add_query_terms(&mut query, "PrivateNetworkRequestPolicy", 1.0);

    let candidates = index.search(&query, "repo", 0);

    assert_eq!(candidates[0].path, Path::new("repo/src/network.rs"));
    assert_eq!(candidates[0].row_range, 0..1);
    assert_eq!(candidates[0].order, 0);
}

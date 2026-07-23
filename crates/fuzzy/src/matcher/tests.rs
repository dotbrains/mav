
use util::rel_path::{RelPath, rel_path};

use crate::{PathMatch, PathMatchCandidate};

use super::*;
use std::sync::Arc;

#[test]
fn test_get_last_positions() {
    let mut query: &[char] = &['d', 'c'];
    let mut matcher = Matcher::new(query, query, query.into(), false, true);
    let result = matcher.find_last_positions(&['a', 'b', 'c'], &['b', 'd', 'e', 'f']);
    assert!(!result);

    query = &['c', 'd'];
    let mut matcher = Matcher::new(query, query, query.into(), false, true);
    let result = matcher.find_last_positions(&['a', 'b', 'c'], &['b', 'd', 'e', 'f']);
    assert!(result);
    assert_eq!(matcher.last_positions, vec![2, 4]);

    query = &['z', '/', 'z', 'f'];
    let mut matcher = Matcher::new(query, query, query.into(), false, true);
    let result = matcher.find_last_positions(&['z', 'e', 'd', '/'], &['z', 'e', 'd', '/', 'f']);
    assert!(result);
    assert_eq!(matcher.last_positions, vec![0, 3, 4, 8]);
}

#[test]
fn test_match_path_entries() {
    let paths = vec![
        "",
        "a",
        "ab",
        "abC",
        "abcd",
        "alphabravocharlie",
        "AlphaBravoCharlie",
        "thisisatestdir",
        "ThisIsATestDir",
        "this/is/a/test/dir",
        "test/tiatd",
    ];

    assert_eq!(
        match_single_path_query("abc", false, &paths),
        vec![
            ("abC", vec![0, 1, 2]),
            ("abcd", vec![0, 1, 2]),
            ("AlphaBravoCharlie", vec![0, 5, 10]),
            ("alphabravocharlie", vec![4, 5, 10]),
        ]
    );
    assert_eq!(
        match_single_path_query("t/i/a/t/d", false, &paths),
        vec![("this/is/a/test/dir", vec![0, 4, 5, 7, 8, 9, 10, 14, 15]),]
    );

    assert_eq!(
        match_single_path_query("tiatd", false, &paths),
        vec![
            ("test/tiatd", vec![5, 6, 7, 8, 9]),
            ("ThisIsATestDir", vec![0, 4, 6, 7, 11]),
            ("this/is/a/test/dir", vec![0, 5, 8, 10, 15]),
            ("thisisatestdir", vec![0, 2, 6, 7, 11]),
        ]
    );
}

#[test]
fn test_lowercase_longer_than_uppercase() {
    // This character has more chars in lower-case than in upper-case.
    let paths = vec!["\u{0130}"];
    let query = "\u{0130}";
    assert_eq!(
        match_single_path_query(query, false, &paths),
        vec![("\u{0130}", vec![0])]
    );

    // Path is the lower-case version of the query
    let paths = vec!["i\u{307}"];
    let query = "\u{0130}";
    assert_eq!(
        match_single_path_query(query, false, &paths),
        vec![("i\u{307}", vec![0])]
    );
}

#[test]
fn test_match_multibyte_path_entries() {
    let paths = vec![
        "aαbβ/cγdδ",
        "αβγδ/bcde",
        "c1️⃣2️⃣3️⃣/d4️⃣5️⃣6️⃣/e7️⃣8️⃣9️⃣/f",
        "d/🆒/h",
    ];
    assert_eq!("1️⃣".len(), 7);
    assert_eq!(
        match_single_path_query("bcd", false, &paths),
        vec![
            ("αβγδ/bcde", vec![9, 10, 11]),
            ("aαbβ/cγdδ", vec![3, 7, 10]),
        ]
    );
    assert_eq!(
        match_single_path_query("cde", false, &paths),
        vec![
            ("αβγδ/bcde", vec![10, 11, 12]),
            ("c1️⃣2️⃣3️⃣/d4️⃣5️⃣6️⃣/e7️⃣8️⃣9️⃣/f", vec![0, 23, 46]),
        ]
    );
}

#[test]
fn match_unicode_path_entries() {
    let mixed_unicode_paths = vec![
        "İolu/oluş",
        "İstanbul/code",
        "Athens/Şanlıurfa",
        "Çanakkale/scripts",
        "paris/Düzce_İl",
        "Berlin_Önemli_Ğündem",
        "KİTAPLIK/london/dosya",
        "tokyo/kyoto/fuji",
        "new_york/san_francisco",
    ];

    assert_eq!(
        match_single_path_query("İo/oluş", false, &mixed_unicode_paths),
        vec![("İolu/oluş", vec![0, 2, 5, 6, 7, 8, 9])]
    );

    assert_eq!(
        match_single_path_query("İst/code", false, &mixed_unicode_paths),
        vec![("İstanbul/code", vec![0, 2, 3, 9, 10, 11, 12, 13])]
    );

    assert_eq!(
        match_single_path_query("athens/şa", false, &mixed_unicode_paths),
        vec![("Athens/Şanlıurfa", vec![0, 1, 2, 3, 4, 5, 6, 7, 9])]
    );

    assert_eq!(
        match_single_path_query("BerlinÖĞ", false, &mixed_unicode_paths),
        vec![("Berlin_Önemli_Ğündem", vec![0, 1, 2, 3, 4, 5, 7, 15])]
    );

    assert_eq!(
        match_single_path_query("tokyo/fuji", false, &mixed_unicode_paths),
        vec![("tokyo/kyoto/fuji", vec![0, 1, 2, 3, 4, 5, 12, 13, 14, 15])]
    );

    let mixed_script_paths = vec![
        "résumé_Москва",
        "naïve_київ_implementation",
        "café_北京_app",
        "東京_über_driver",
        "déjà_vu_cairo",
        "seoul_piñata_game",
        "voilà_istanbul_result",
    ];

    assert_eq!(
        match_single_path_query("résmé", false, &mixed_script_paths),
        vec![("résumé_Москва", vec![0, 1, 3, 5, 6])]
    );

    assert_eq!(
        match_single_path_query("café北京", false, &mixed_script_paths),
        vec![("café_北京_app", vec![0, 1, 2, 3, 6, 9])]
    );

    assert_eq!(
        match_single_path_query("ista", false, &mixed_script_paths),
        vec![("voilà_istanbul_result", vec![7, 8, 9, 10])]
    );

    let complex_paths = vec![
        "document_📚_library",
        "project_👨‍👩‍👧‍👦_family",
        "flags_🇯🇵🇺🇸🇪🇺_world",
        "code_😀😃😄😁_happy",
        "photo_👩‍👩‍👧‍👦_album",
    ];

    assert_eq!(
        match_single_path_query("doc📚lib", false, &complex_paths),
        vec![("document_📚_library", vec![0, 1, 2, 9, 14, 15, 16])]
    );

    assert_eq!(
        match_single_path_query("codehappy", false, &complex_paths),
        vec![("code_😀😃😄😁_happy", vec![0, 1, 2, 3, 22, 23, 24, 25, 26])]
    );
}

#[test]
fn test_positions_are_valid_char_boundaries_with_expanding_lowercase() {
    // İ (U+0130) lowercases to "i\u{307}" (2 chars) under full case folding.
    // With simple case mapping (used by this matcher), İ → 'i' (1 char),
    // so positions remain valid byte boundaries.
    let paths = vec!["İstanbul/code.rs", "aİbİc/dİeİf.txt", "src/İmport/İndex.ts"];

    for query in &["code", "İst", "dİe", "İndex", "İmport", "abcdef"] {
        let results = match_single_path_query(query, false, &paths);
        for (path, positions) in &results {
            for &pos in positions {
                assert!(
                    path.is_char_boundary(pos),
                    "Position {pos} is not a valid char boundary in path {path:?} \
                         (query: {query:?}, all positions: {positions:?})"
                );
            }
        }
    }
}

#[test]
fn test_positions_valid_with_various_multibyte_chars() {
    // German ß uppercases to SS but lowercases to itself — no expansion.
    // Armenian ligatures and other characters that could expand under full
    // case folding should still produce valid byte boundaries.
    let paths = vec![
        "straße/config.rs",
        "Straße/München/file.txt",
        "ﬁle/path.rs",     // ﬁ (U+FB01, fi ligature)
        "ﬀoo/bar.txt",     // ﬀ (U+FB00, ff ligature)
        "aÇbŞc/dÖeÜf.txt", // Turkish chars that don't expand
    ];

    for query in &["config", "Mün", "file", "bar", "abcdef", "straße", "ÇŞ"] {
        let results = match_single_path_query(query, false, &paths);
        for (path, positions) in &results {
            for &pos in positions {
                assert!(
                    path.is_char_boundary(pos),
                    "Position {pos} is not a valid char boundary in path {path:?} \
                         (query: {query:?}, all positions: {positions:?})"
                );
            }
        }
    }
}

fn match_single_path_query<'a>(
    query: &str,
    smart_case: bool,
    paths: &[&'a str],
) -> Vec<(&'a str, Vec<usize>)> {
    let lowercase_query = query.chars().map(simple_lowercase).collect::<Vec<_>>();
    let query = query.chars().collect::<Vec<_>>();
    let query_chars = CharBag::from(&lowercase_query[..]);

    let path_arcs: Vec<Arc<RelPath>> = paths
        .iter()
        .map(|path| Arc::from(rel_path(path)))
        .collect::<Vec<_>>();
    let mut path_entries = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let lowercase_path: Vec<char> = path.chars().map(simple_lowercase).collect();
        let char_bag = CharBag::from(lowercase_path.as_slice());
        path_entries.push(PathMatchCandidate {
            is_dir: false,
            char_bag,
            path: &path_arcs[i],
        });
    }

    let mut matcher = Matcher::new(&query, &lowercase_query, query_chars, smart_case, true);

    let cancel_flag = AtomicBool::new(false);
    let mut results = Vec::new();

    matcher.match_candidates(
        &[],
        &[],
        path_entries.into_iter(),
        &mut results,
        &cancel_flag,
        |candidate, score, positions| PathMatch {
            score,
            worktree_id: 0,
            positions: positions.clone(),
            path: candidate.path.into(),
            path_prefix: RelPath::empty_arc(),
            distance_to_relative_ancestor: usize::MAX,
            is_dir: false,
        },
    );
    results.sort_by(|a, b| b.cmp(a));

    results
        .into_iter()
        .map(|result| {
            (
                paths
                    .iter()
                    .copied()
                    .find(|p| result.path.as_ref() == rel_path(p))
                    .unwrap(),
                result.positions,
            )
        })
        .collect()
}

/// Test for https://github.com/mav-industries/mav/issues/44324
#[test]
fn test_recursive_score_match_index_out_of_bounds() {
    let paths = vec!["İ/İ/İ/İ"];
    let query = "İ/İ";

    // This panicked with "index out of bounds: the len is 21 but the index is 22"
    let result = match_single_path_query(query, false, &paths);
    let _ = result;
}

use super::*;

use gpui::BackgroundExecutor;

fn candidates(strings: &[&str]) -> Vec<StringMatchCandidate> {
    strings
        .iter()
        .enumerate()
        .map(|(id, s)| StringMatchCandidate::new(id, *s))
        .collect()
}

#[gpui::test]
async fn test_basic_match(executor: BackgroundExecutor) {
    let cs = candidates(&["hello", "world", "help"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "hel",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    let matched: Vec<&str> = results.iter().map(|m| m.string.as_ref()).collect();
    assert!(matched.contains(&"hello"));
    assert!(matched.contains(&"help"));
    assert!(!matched.contains(&"world"));
}

#[gpui::test]
async fn test_multi_word_query(executor: BackgroundExecutor) {
    let cs = candidates(&[
        "src/lib/parser.rs",
        "src/bin/main.rs",
        "tests/parser_test.rs",
    ]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "src parser",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].string, "src/lib/parser.rs");
}

#[gpui::test]
async fn test_empty_query_returns_all(executor: BackgroundExecutor) {
    let cs = candidates(&["alpha", "beta", "gamma"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|m| m.score == 0.0));
}

#[gpui::test]
async fn test_whitespace_only_query_returns_all(executor: BackgroundExecutor) {
    let cs = candidates(&["alpha", "beta", "gamma"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "   \t\n",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 3);
}

#[gpui::test]
async fn test_empty_candidates(executor: BackgroundExecutor) {
    let cs: Vec<StringMatchCandidate> = vec![];
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "query",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert!(results.is_empty());
}

#[gpui::test]
async fn test_cancellation(executor: BackgroundExecutor) {
    let cs = candidates(&["hello", "world"]);
    let cancel = AtomicBool::new(true);
    let results = match_strings_async(
        &cs,
        "hel",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert!(results.is_empty());
}

#[gpui::test]
async fn test_max_results_limit(executor: BackgroundExecutor) {
    let cs = candidates(&["ab", "abc", "abcd", "abcde"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "ab",
        Case::Ignore,
        LengthPenalty::Off,
        2,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 2);
}

#[gpui::test]
async fn test_scoring_order(executor: BackgroundExecutor) {
    let cs = candidates(&[
        "some_very_long_variable_name_fuzzy",
        "fuzzy",
        "a_fuzzy_thing",
    ]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "fuzzy",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor.clone(),
    )
    .await;

    let ordered = matches!(
        (
            results[0].string.as_ref(),
            results[1].string.as_ref(),
            results[2].string.as_ref()
        ),
        (
            "fuzzy",
            "a_fuzzy_thing",
            "some_very_long_variable_name_fuzzy"
        )
    );
    assert!(ordered, "matches are not in the proper order.");

    let results_penalty = match_strings_async(
        &cs,
        "fuzzy",
        Case::Ignore,
        LengthPenalty::On,
        10,
        &cancel,
        executor,
    )
    .await;
    let greater = results[2].score > results_penalty[2].score;
    assert!(greater, "penalize length not affecting long candidates");
}

#[gpui::test]
async fn test_utf8_positions(executor: BackgroundExecutor) {
    let cs = candidates(&["café"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "caf",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 1);
    let m = &results[0];
    assert_eq!(m.positions, vec![0, 1, 2]);
    for &pos in &m.positions {
        assert!(m.string.is_char_boundary(pos));
    }
}

#[gpui::test]
async fn test_smart_case(executor: BackgroundExecutor) {
    let cs = candidates(&["FooBar", "foobar", "FOOBAR"]);
    let cancel = AtomicBool::new(false);

    let case_insensitive = match_strings_async(
        &cs,
        "foobar",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor.clone(),
    )
    .await;
    assert_eq!(case_insensitive.len(), 3);

    let smart = match_strings_async(
        &cs,
        "FooBar",
        Case::Smart,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert!(smart.iter().any(|m| m.string == "FooBar"));
    let foobar_score = smart.iter().find(|m| m.string == "FooBar").map(|m| m.score);
    let lower_score = smart.iter().find(|m| m.string == "foobar").map(|m| m.score);
    if let (Some(exact), Some(lower)) = (foobar_score, lower_score) {
        assert!(exact >= lower);
    }
}

#[gpui::test]
async fn test_smart_case_does_not_flip_order_when_length_penalty_on(executor: BackgroundExecutor) {
    // Regression for the sign bug: with a length penalty large enough to push
    // `total_score - length_penalty` negative, case mismatches used to make
    // scores *better* (less negative). Exact-case match must still rank first.
    let cs = candidates(&[
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaa_FooBar",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaa_foobar",
    ]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "FooBar",
        Case::Smart,
        LengthPenalty::On,
        10,
        &cancel,
        executor,
    )
    .await;
    let exact = results
        .iter()
        .find(|m| m.string.as_ref() == "aaaaaaaaaaaaaaaaaaaaaaaaaaaa_FooBar")
        .map(|m| m.score)
        .expect("exact-case candidate should match");
    let mismatch = results
        .iter()
        .find(|m| m.string.as_ref() == "aaaaaaaaaaaaaaaaaaaaaaaaaaaa_foobar")
        .map(|m| m.score)
        .expect("mismatch-case candidate should match");
    assert!(
        exact >= mismatch,
        "exact-case score ({exact}) should be >= mismatch-case score ({mismatch})"
    );
}

#[gpui::test]
async fn test_char_bag_prefilter(executor: BackgroundExecutor) {
    let cs = candidates(&["abcdef", "abc", "def", "aabbcc"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "abc",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    let matched: Vec<&str> = results.iter().map(|m| m.string.as_ref()).collect();
    assert!(matched.contains(&"abcdef"));
    assert!(matched.contains(&"abc"));
    assert!(matched.contains(&"aabbcc"));
    assert!(!matched.contains(&"def"));
}

#[test]
fn test_sync_basic_match() {
    let cs = candidates(&["hello", "world", "help"]);
    let results = match_strings(&cs, "hel", Case::Ignore, LengthPenalty::Off, 10);
    let matched: Vec<&str> = results.iter().map(|m| m.string.as_ref()).collect();
    assert!(matched.contains(&"hello"));
    assert!(matched.contains(&"help"));
    assert!(!matched.contains(&"world"));
}

#[test]
fn test_sync_empty_query_returns_all() {
    let cs = candidates(&["alpha", "beta", "gamma"]);
    let results = match_strings(&cs, "", Case::Ignore, LengthPenalty::Off, 10);
    assert_eq!(results.len(), 3);
}

#[test]
fn test_sync_whitespace_only_query_returns_all() {
    let cs = candidates(&["alpha", "beta", "gamma"]);
    let results = match_strings(&cs, "  ", Case::Ignore, LengthPenalty::Off, 10);
    assert_eq!(results.len(), 3);
}

#[test]
fn test_sync_max_results() {
    let cs = candidates(&["ab", "abc", "abcd", "abcde"]);
    let results = match_strings(&cs, "ab", Case::Ignore, LengthPenalty::Off, 2);
    assert_eq!(results.len(), 2);
}

#[gpui::test]
async fn test_empty_query_respects_max_results(executor: BackgroundExecutor) {
    let cs = candidates(&["alpha", "beta", "gamma", "delta"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "",
        Case::Ignore,
        LengthPenalty::Off,
        2,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 2);
}

#[gpui::test]
async fn test_multi_word_with_nonmatching_word(executor: BackgroundExecutor) {
    let cs = candidates(&["src/parser.rs", "src/main.rs"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "src xyzzy",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert!(
        results.is_empty(),
        "no candidate contains 'xyzzy', so nothing should match"
    );
}

#[gpui::test]
async fn test_segment_size_not_divisible_by_cpus(executor: BackgroundExecutor) {
    executor.set_num_cpus(4);
    let cs = candidates(&["alpha", "beta", "gamma", "delta", "epsilon"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "a",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    let matched: Vec<&str> = results.iter().map(|m| m.string.as_ref()).collect();
    assert!(matched.contains(&"alpha"));
    assert!(matched.contains(&"gamma"));
    assert!(matched.contains(&"delta"));
}

#[gpui::test]
async fn test_segment_size_with_many_cpus_few_candidates(executor: BackgroundExecutor) {
    executor.set_num_cpus(16);
    let cs = candidates(&["one", "two", "three"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "o",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    let matched: Vec<&str> = results.iter().map(|m| m.string.as_ref()).collect();
    assert!(matched.contains(&"one"));
    assert!(matched.contains(&"two"));
}

#[gpui::test]
async fn test_segment_size_single_candidate(executor: BackgroundExecutor) {
    executor.set_num_cpus(8);
    let cs = candidates(&["lonely"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "lone",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].string.as_ref(), "lonely");
}

#[gpui::test]
async fn test_segment_size_candidates_equal_cpus(executor: BackgroundExecutor) {
    executor.set_num_cpus(4);
    let cs = candidates(&["aaa", "bbb", "ccc", "ddd"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "a",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].string.as_ref(), "aaa");
}

#[gpui::test]
async fn test_segment_size_candidates_one_more_than_cpus(executor: BackgroundExecutor) {
    executor.set_num_cpus(3);
    let cs = candidates(&["ant", "ape", "dog", "axe"]);
    let cancel = AtomicBool::new(false);
    let results = match_strings_async(
        &cs,
        "a",
        Case::Ignore,
        LengthPenalty::Off,
        10,
        &cancel,
        executor,
    )
    .await;
    let matched: Vec<&str> = results.iter().map(|m| m.string.as_ref()).collect();
    assert!(matched.contains(&"ant"));
    assert!(matched.contains(&"ape"));
    assert!(matched.contains(&"axe"));
    assert!(!matched.contains(&"dog"));
}

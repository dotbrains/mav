use super::*;

use crate::{Buffer, ContextProvider, Language, LanguageConfig, LanguageMatcher, LanguageQueries};
use gpui::{AppContext as _, TestAppContext};
use indoc::indoc;
use std::{borrow::Cow, sync::Arc};

struct TestContextProvider {
    resolver: Arc<dyn RunnableResolver>,
}

impl ContextProvider for TestContextProvider {
    fn runnable_resolver(&self) -> Option<Arc<dyn RunnableResolver>> {
        Some(self.resolver.clone())
    }
}

fn make_language(
    runnables_query: &'static str,
    resolver: Option<Arc<dyn RunnableResolver>>,
) -> Arc<Language> {
    let language = Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        runnables: Some(Cow::Borrowed(runnables_query)),
        ..Default::default()
    })
    .expect("parse runnables query");
    let context_provider = resolver
        .map(|resolver| Arc::new(TestContextProvider { resolver }) as Arc<dyn ContextProvider>);
    Arc::new(language.with_context_provider(context_provider))
}

fn collect_runnables(
    cx: &mut TestAppContext,
    source: &str,
    runnables_query: &'static str,
    resolver: Option<Arc<dyn RunnableResolver>>,
) -> Vec<RunnableRange> {
    collect_runnables_in(cx, source, runnables_query, resolver, None)
}

fn collect_runnables_in(
    cx: &mut TestAppContext,
    source: &str,
    runnables_query: &'static str,
    resolver: Option<Arc<dyn RunnableResolver>>,
    offset_range: Option<Range<usize>>,
) -> Vec<RunnableRange> {
    let language = make_language(runnables_query, resolver);
    let source_owned = source.to_string();
    let buffer =
        cx.new(|cx| Buffer::local(source_owned.clone(), cx).with_language(language.clone(), cx));
    cx.executor().run_until_parked();
    let range = offset_range.unwrap_or(0..source_owned.len());
    buffer.update(cx, |buffer, _| {
        buffer.snapshot().runnable_ranges(range).collect()
    })
}

fn text_at(buffer: &BufferSnapshot, range: Range<usize>) -> String {
    buffer.text_for_range(range).collect()
}

/// Picks the first `@run` capture, attaches no extras.
struct FirstRunResolver;

impl RunnableResolver for FirstRunResolver {
    fn resolve(
        &self,
        local_captures: &[RunnableMatchCapture],
        _shared_captures: &[RunnableMatchCapture],
        _buffer: &BufferSnapshot,
    ) -> Option<ResolvedRunnable> {
        let run = local_captures.iter().find(|capture| capture.is_run())?;
        Some(ResolvedRunnable {
            run_range: run.range(),
            extra_captures: SmallVec::new(),
        })
    }
}

/// Picks the first `@run` and surfaces every local named capture as an extra.
struct LocalExtrasResolver;

impl RunnableResolver for LocalExtrasResolver {
    fn resolve(
        &self,
        local_captures: &[RunnableMatchCapture],
        _shared_captures: &[RunnableMatchCapture],
        buffer: &BufferSnapshot,
    ) -> Option<ResolvedRunnable> {
        let run = local_captures.iter().find(|capture| capture.is_run())?;
        let extras = local_captures
            .iter()
            .filter_map(|capture| {
                capture
                    .name()
                    .map(|name| (name.to_string(), text_at(buffer, capture.range())))
            })
            .collect();
        Some(ResolvedRunnable {
            run_range: run.range(),
            extra_captures: extras,
        })
    }
}

/// Skips groups whose `@run` text equals `skip_text`; otherwise picks the first `@run`.
struct SkipByTextResolver {
    skip_text: &'static str,
}

impl RunnableResolver for SkipByTextResolver {
    fn resolve(
        &self,
        local_captures: &[RunnableMatchCapture],
        _shared_captures: &[RunnableMatchCapture],
        buffer: &BufferSnapshot,
    ) -> Option<ResolvedRunnable> {
        let run = local_captures.iter().find(|capture| capture.is_run())?;
        if text_at(buffer, run.range()) == self.skip_text {
            return None;
        }
        Some(ResolvedRunnable {
            run_range: run.range(),
            extra_captures: SmallVec::new(),
        })
    }
}

/// Always emits `_outer = LOCAL` as a local extra (to exercise the shared/local merge).
struct OverrideSharedResolver;

impl RunnableResolver for OverrideSharedResolver {
    fn resolve(
        &self,
        local_captures: &[RunnableMatchCapture],
        _shared_captures: &[RunnableMatchCapture],
        _buffer: &BufferSnapshot,
    ) -> Option<ResolvedRunnable> {
        let run = local_captures.iter().find(|capture| capture.is_run())?;
        let mut extras: SmallVec<[(String, String); 2]> = SmallVec::new();
        extras.push(("_outer".to_string(), "LOCAL".to_string()));
        Some(ResolvedRunnable {
            run_range: run.range(),
            extra_captures: extras,
        })
    }
}

const GROUPED_QUERY: &str = indoc! {r#"
        (function_item
          name: (identifier) @_outer
          body: (block
            ((expression_statement
               (call_expression
                 function: (identifier) @run @_call)) @run_item)+))
    "#};

const GROUPED_SOURCE: &str = indoc! {r#"
        fn outer() {
            alpha();
            beta();
            gamma();
        }
    "#};

#[gpui::test]
fn test_single_match_emits_one_runnable_per_match(cx: &mut TestAppContext) {
    let query = indoc! {r#"
            ((function_item
               name: (identifier) @run
               (#match? @run "^test_")) @_decl)
        "#};
    let source = indoc! {r#"
            fn test_alpha() {}
            fn helper() {}
            fn test_beta() {}
        "#};

    let runnables = collect_runnables(cx, source, query, None);
    let run_texts: Vec<String> = runnables
        .iter()
        .map(|range| source[range.run_range.clone()].to_string())
        .collect();
    assert_eq!(run_texts, vec!["test_alpha", "test_beta"]);

    let decls: Vec<&str> = runnables
        .iter()
        .filter_map(|range| range.extra_captures.get("_decl").map(String::as_str))
        .collect();
    assert_eq!(decls, vec!["fn test_alpha() {}", "fn test_beta() {}"]);
}

#[gpui::test]
fn test_single_match_without_run_capture_skipped(cx: &mut TestAppContext) {
    // Pattern with only a named capture and no `@run`: should silently produce nothing.
    let query = indoc! {r#"
            (function_item) @_decl
        "#};
    let source = indoc! {r#"
            fn helper() {}
            fn another() {}
        "#};

    let runnables = collect_runnables(cx, source, query, None);
    assert!(
        runnables.is_empty(),
        "matches without @run should produce no runnables, got {}",
        runnables.len()
    );
}

#[gpui::test]
fn test_match_with_no_runnable_does_not_terminate_iteration(cx: &mut TestAppContext) {
    // A syntax match yielding no runnable must not terminate the
    // outer iterator before later matches that DO have `@run` are visited.
    let query = indoc! {r#"
            ((function_item
               name: (identifier) @_helper
               (#match? @_helper "^helper")) @_decl_no_run)

            ((function_item
               name: (identifier) @run
               (#match? @run "^test_")) @_decl)
        "#};
    let source = indoc! {r#"
            fn helper() {}
            fn test_alpha() {}
        "#};

    let runnables = collect_runnables(cx, source, query, None);
    let run_texts: Vec<String> = runnables
        .iter()
        .map(|range| source[range.run_range.clone()].to_string())
        .collect();
    assert_eq!(
        run_texts,
        vec!["test_alpha"],
        "syntax matches that produce no runnable must not terminate iteration"
    );
}

#[gpui::test]
fn test_grouped_match_without_resolver_emits_nothing(cx: &mut TestAppContext) {
    // `@run_item` is present but no resolver is registered on the language.
    let runnables = collect_runnables(cx, GROUPED_SOURCE, GROUPED_QUERY, None);
    assert!(
        runnables.is_empty(),
        "grouped path with no resolver should emit nothing, got {}",
        runnables.len()
    );
}

#[gpui::test]
fn test_grouped_match_emits_one_runnable_per_run_item(cx: &mut TestAppContext) {
    let resolver: Arc<dyn RunnableResolver> = Arc::new(FirstRunResolver);
    let runnables = collect_runnables(cx, GROUPED_SOURCE, GROUPED_QUERY, Some(resolver));

    let run_texts: Vec<String> = runnables
        .iter()
        .map(|range| GROUPED_SOURCE[range.run_range.clone()].to_string())
        .collect();
    assert_eq!(run_texts, vec!["alpha", "beta", "gamma"]);
}

#[gpui::test]
fn test_grouped_match_shared_captures_propagate(cx: &mut TestAppContext) {
    let resolver: Arc<dyn RunnableResolver> = Arc::new(FirstRunResolver);
    let runnables = collect_runnables(cx, GROUPED_SOURCE, GROUPED_QUERY, Some(resolver));

    for range in &runnables {
        assert_eq!(
            range.extra_captures.get("_outer").map(String::as_str),
            Some("outer"),
            "every grouped runnable should inherit the shared `_outer` capture"
        );
    }
    assert_eq!(runnables.len(), 3);
}

#[gpui::test]
fn test_grouped_match_local_extras_are_per_group(cx: &mut TestAppContext) {
    let resolver: Arc<dyn RunnableResolver> = Arc::new(LocalExtrasResolver);
    let runnables = collect_runnables(cx, GROUPED_SOURCE, GROUPED_QUERY, Some(resolver));

    let calls: Vec<&str> = runnables
        .iter()
        .filter_map(|range| range.extra_captures.get("_call").map(String::as_str))
        .collect();
    assert_eq!(
        calls,
        vec!["alpha", "beta", "gamma"],
        "each group's local `_call` capture should come from that row only"
    );
}

#[gpui::test]
fn test_grouped_match_resolver_returning_none_skips_group(cx: &mut TestAppContext) {
    let source = indoc! {r#"
            fn outer() {
                alpha();
                skip_me();
                gamma();
            }
        "#};
    let resolver: Arc<dyn RunnableResolver> = Arc::new(SkipByTextResolver {
        skip_text: "skip_me",
    });
    let runnables = collect_runnables(cx, source, GROUPED_QUERY, Some(resolver));

    let run_texts: Vec<String> = runnables
        .iter()
        .map(|range| source[range.run_range.clone()].to_string())
        .collect();
    assert_eq!(run_texts, vec!["alpha", "gamma"]);
}

#[gpui::test]
fn test_grouped_match_offset_range_filters_groups(cx: &mut TestAppContext) {
    let resolver: Arc<dyn RunnableResolver> = Arc::new(FirstRunResolver);
    let beta_offset = GROUPED_SOURCE
        .find("beta()")
        .expect("source should contain `beta()`");
    let runnables = collect_runnables_in(
        cx,
        GROUPED_SOURCE,
        GROUPED_QUERY,
        Some(resolver),
        Some(beta_offset..beta_offset + "beta".len()),
    );

    let run_texts: Vec<String> = runnables
        .iter()
        .map(|range| GROUPED_SOURCE[range.run_range.clone()].to_string())
        .collect();
    assert_eq!(
        run_texts,
        vec!["beta"],
        "offset_range should restrict emitted groups to those overlapping it"
    );
}

#[gpui::test]
fn test_grouped_match_zero_width_offset_at_group_start(cx: &mut TestAppContext) {
    let resolver: Arc<dyn RunnableResolver> = Arc::new(FirstRunResolver);
    let alpha_offset = GROUPED_SOURCE
        .find("alpha()")
        .expect("source should contain `alpha()`");
    let runnables = collect_runnables_in(
        cx,
        GROUPED_SOURCE,
        GROUPED_QUERY,
        Some(resolver),
        Some(alpha_offset..alpha_offset),
    );

    let run_texts: Vec<String> = runnables
        .iter()
        .map(|range| GROUPED_SOURCE[range.run_range.clone()].to_string())
        .collect();
    assert_eq!(
        run_texts,
        vec!["alpha"],
        "zero-width offset_range at the start of a group should include that group"
    );
}

#[gpui::test]
fn test_local_extras_override_shared_extras_with_same_key(cx: &mut TestAppContext) {
    let resolver: Arc<dyn RunnableResolver> = Arc::new(OverrideSharedResolver);
    let runnables = collect_runnables(cx, GROUPED_SOURCE, GROUPED_QUERY, Some(resolver));

    for range in &runnables {
        assert_eq!(
            range.extra_captures.get("_outer").map(String::as_str),
            Some("LOCAL"),
            "local extras should override shared extras with the same key"
        );
    }
    assert_eq!(runnables.len(), 3);
}

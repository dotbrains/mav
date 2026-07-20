use collections::{BTreeSet, HashMap};
use itertools::Itertools as _;
use language::{
    BufferSnapshot, CachedLspAdapter, CodeLabel, CodeLabelExt, Language, LanguageAwareStyling,
    LanguageRegistry,
};
use std::{iter, mem, path::Path, sync::Arc};
use util::ResultExt as _;

use crate::{
    Completion, CompletionSource, CoreCompletion, Hover, Symbol,
    lsp_store::symbol_types::CoreSymbol,
};

pub(crate) fn resolve_word_completion(snapshot: &BufferSnapshot, completion: &mut Completion) {
    let CompletionSource::BufferWord {
        word_range,
        resolved,
    } = &mut completion.source
    else {
        return;
    };
    if *resolved {
        return;
    }

    if completion.new_text
        != snapshot
            .text_for_range(word_range.clone())
            .collect::<String>()
    {
        return;
    }

    let mut offset = 0;
    for chunk in snapshot.chunks(
        word_range.clone(),
        LanguageAwareStyling {
            tree_sitter: true,
            diagnostics: true,
        },
    ) {
        let end_offset = offset + chunk.text.len();
        if let Some(highlight_id) = chunk.syntax_highlight_id {
            completion
                .label
                .runs
                .push((offset..end_offset, highlight_id));
        }
        offset = end_offset;
    }
    *resolved = true;
}

pub(crate) fn remove_empty_hover_blocks(mut hover: Hover) -> Option<Hover> {
    hover
        .contents
        .retain(|hover_block| !hover_block.text.trim().is_empty());
    if hover.contents.is_empty() {
        None
    } else {
        Some(hover)
    }
}

pub(crate) async fn populate_labels_for_completions(
    new_completions: Vec<CoreCompletion>,
    language: Option<Arc<Language>>,
    lsp_adapter: Option<Arc<CachedLspAdapter>>,
) -> Vec<Completion> {
    let lsp_completions = new_completions
        .iter()
        .filter_map(|new_completion| {
            new_completion
                .source
                .lsp_completion(true)
                .map(|lsp_completion| lsp_completion.into_owned())
        })
        .collect::<Vec<_>>();

    let mut labels = if let Some((language, lsp_adapter)) = language.as_ref().zip(lsp_adapter) {
        lsp_adapter
            .labels_for_completions(&lsp_completions, language)
            .await
            .log_err()
            .unwrap_or_default()
    } else {
        Vec::new()
    }
    .into_iter()
    .fuse();

    let mut completions = Vec::new();
    for completion in new_completions {
        match completion.source.lsp_completion(true) {
            Some(lsp_completion) => {
                let documentation = lsp_completion.documentation.clone().map(|docs| docs.into());

                let mut label = labels.next().flatten().unwrap_or_else(|| {
                    CodeLabel::fallback_for_completion(&lsp_completion, language.as_deref())
                });
                ensure_uniform_list_compatible_label(&mut label);
                completions.push(Completion {
                    label,
                    documentation,
                    replace_range: completion.replace_range,
                    new_text: completion.new_text,
                    insert_text_mode: lsp_completion.insert_text_mode,
                    source: completion.source,
                    icon_path: None,
                    icon_color: None,
                    confirm: None,
                    match_start: None,
                    snippet_deduplication_key: None,
                    group: None,
                });
            }
            None => {
                let mut label = CodeLabel::plain(completion.new_text.clone(), None);
                ensure_uniform_list_compatible_label(&mut label);
                completions.push(Completion {
                    label,
                    documentation: None,
                    replace_range: completion.replace_range,
                    new_text: completion.new_text,
                    source: completion.source,
                    insert_text_mode: None,
                    icon_path: None,
                    icon_color: None,
                    confirm: None,
                    match_start: None,
                    snippet_deduplication_key: None,
                    group: None,
                });
            }
        }
    }
    completions
}

pub(crate) async fn populate_labels_for_symbols(
    symbols: Vec<CoreSymbol>,
    language_registry: &Arc<LanguageRegistry>,
    lsp_adapter: Option<Arc<CachedLspAdapter>>,
    output: &mut Vec<Symbol>,
) {
    #[allow(clippy::mutable_key_type)]
    let mut symbols_by_language = HashMap::<Option<Arc<Language>>, Vec<CoreSymbol>>::default();

    let mut unknown_paths = BTreeSet::<Arc<str>>::new();
    for symbol in symbols {
        let Some(file_name) = symbol.path.file_name() else {
            continue;
        };
        let language = language_registry
            .load_language_for_file_path(Path::new(file_name))
            .await
            .ok()
            .or_else(|| {
                unknown_paths.insert(file_name.into());
                None
            });
        symbols_by_language
            .entry(language)
            .or_default()
            .push(symbol);
    }

    for unknown_path in unknown_paths {
        log::info!("no language found for symbol in file {unknown_path:?}");
    }

    let mut label_params = Vec::new();
    for (language, mut symbols) in symbols_by_language {
        label_params.clear();
        label_params.extend(symbols.iter_mut().map(|symbol| language::Symbol {
            name: mem::take(&mut symbol.name),
            kind: symbol.kind,
            container_name: symbol.container_name.take(),
        }));

        let mut labels = Vec::new();
        if let Some(language) = language {
            let lsp_adapter = lsp_adapter.clone().or_else(|| {
                language_registry
                    .lsp_adapters(&language.name())
                    .first()
                    .cloned()
            });
            if let Some(lsp_adapter) = lsp_adapter {
                labels = lsp_adapter
                    .labels_for_symbols(&label_params, &language)
                    .await
                    .log_err()
                    .unwrap_or_default();
            }
        }

        for (
            (
                symbol,
                language::Symbol {
                    name,
                    container_name,
                    ..
                },
            ),
            label,
        ) in symbols
            .into_iter()
            .zip(label_params.drain(..))
            .zip(labels.into_iter().chain(iter::repeat(None)))
        {
            output.push(Symbol {
                language_server_name: symbol.language_server_name,
                source_worktree_id: symbol.source_worktree_id,
                source_language_server_id: symbol.source_language_server_id,
                path: symbol.path,
                label: label.unwrap_or_else(|| CodeLabel::plain(name.clone(), None)),
                name,
                kind: symbol.kind,
                range: symbol.range,
                container_name,
            });
        }
    }
}

pub(crate) fn collapse_newlines(text: &str, separator: &str) -> String {
    text.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .join(separator)
}

/// Completion items are displayed in a `UniformList`.
/// Usually, those items are single-line strings, but in LSP responses,
/// completion items `label`, `detail` and `label_details.description` may contain newlines or long spaces.
/// Many language plugins construct these items by joining these parts together, and we may use `CodeLabel::fallback_for_completion` that uses `label` at least.
/// All that may lead to a newline being inserted into resulting `CodeLabel.text`, which will force `UniformList` to bloat each entry to occupy more space,
/// breaking the completions menu presentation.
///
/// Sanitize the text to ensure there are no newlines, or, if there are some, remove them and also remove long space sequences if there were newlines.
pub(crate) fn ensure_uniform_list_compatible_label(label: &mut CodeLabel) {
    let mut new_text = String::with_capacity(label.text.len());
    let mut offset_map = vec![0; label.text.len() + 1];
    let mut last_char_was_space = false;
    let mut new_idx = 0;
    let chars = label.text.char_indices().fuse();
    let mut newlines_removed = false;

    for (idx, c) in chars {
        offset_map[idx] = new_idx;

        match c {
            '\n' if last_char_was_space => {
                newlines_removed = true;
            }
            '\t' | ' ' if last_char_was_space => {}
            '\n' if !last_char_was_space => {
                new_text.push(' ');
                new_idx += 1;
                last_char_was_space = true;
                newlines_removed = true;
            }
            ' ' | '\t' => {
                new_text.push(' ');
                new_idx += 1;
                last_char_was_space = true;
            }
            _ => {
                new_text.push(c);
                new_idx += c.len_utf8();
                last_char_was_space = false;
            }
        }
    }
    offset_map[label.text.len()] = new_idx;

    if !newlines_removed {
        return;
    }

    let last_index = new_idx;
    let mut run_ranges_errors = Vec::new();
    label.runs.retain_mut(|(range, _)| {
        match offset_map.get(range.start) {
            Some(&start) => range.start = start,
            None => {
                run_ranges_errors.push(range.clone());
                return false;
            }
        }

        match offset_map.get(range.end) {
            Some(&end) => range.end = end,
            None => {
                run_ranges_errors.push(range.clone());
                range.end = last_index;
            }
        }
        true
    });
    if !run_ranges_errors.is_empty() {
        log::error!(
            "Completion label has errors in its run ranges: {run_ranges_errors:?}, label text: {}",
            label.text
        );
    }

    let mut wrong_filter_range = None;
    if label.filter_range == (0..label.text.len()) {
        label.filter_range = 0..new_text.len();
    } else {
        let mut original_filter_range = Some(label.filter_range.clone());
        match offset_map.get(label.filter_range.start) {
            Some(&start) => label.filter_range.start = start,
            None => {
                wrong_filter_range = original_filter_range.take();
                label.filter_range.start = last_index;
            }
        }

        match offset_map.get(label.filter_range.end) {
            Some(&end) => label.filter_range.end = end,
            None => {
                wrong_filter_range = original_filter_range.take();
                label.filter_range.end = last_index;
            }
        }
    }
    if let Some(wrong_filter_range) = wrong_filter_range {
        log::error!(
            "Completion label has an invalid filter range: {wrong_filter_range:?}, label text: {}",
            label.text
        );
    }

    label.text = new_text;
}

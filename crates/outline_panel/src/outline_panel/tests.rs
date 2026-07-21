use db::indoc;
use futures::stream::StreamExt as _;
use gpui::{TestAppContext, UpdateGlobal, VisualTestContext, WindowHandle};
use language::{self, FakeLspAdapter, markdown_lang, rust_lang};
use project::FakeFs;
use search::{
    buffer_search,
    project_search::{self, perform_project_search},
};
use serde_json::json;
use util::path;
use workspace::{MultiWorkspace, OpenOptions, OpenVisible, ToolbarItemView};

use super::*;

#[path = "tests/buffer_search_tests.rs"]
mod buffer_search_tests;
#[path = "tests/click_toggle_behavior.rs"]
mod click_toggle_behavior;
#[path = "tests/expand_collapse_all.rs"]
mod expand_collapse_all;
#[path = "tests/frontend_repo_structure.rs"]
mod frontend_repo_structure;
#[path = "tests/keyboard_expand_collapse.rs"]
mod keyboard_expand_collapse;
#[path = "tests/lsp_document_symbols.rs"]
mod lsp_document_symbols;
#[path = "tests/markdown_heading_boundaries.rs"]
mod markdown_heading_boundaries;
#[path = "tests/multiple_worktrees.rs"]
mod multiple_worktrees;
#[path = "tests/singleton_navigation.rs"]
mod singleton_navigation;

#[path = "tests/item_filtering.rs"]
mod item_filtering;
#[path = "tests/item_opening.rs"]
mod item_opening;
#[path = "tests/project_search_results.rs"]
mod project_search_results;

const SELECTED_MARKER: &str = "  <==== selected";

async fn add_outline_panel(
    project: &Entity<Project>,
    cx: &mut TestAppContext,
) -> (WindowHandle<MultiWorkspace>, Entity<Workspace>) {
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let workspace_weak = workspace.downgrade();
    let outline_panel = window
        .update(cx, |_, window, cx| {
            cx.spawn_in(window, async move |_this, cx| {
                OutlinePanel::load(workspace_weak, cx.clone()).await
            })
        })
        .unwrap()
        .await
        .expect("Failed to load outline panel");

    window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.add_panel(outline_panel, window, cx);
            });
        })
        .unwrap();
    (window, workspace)
}

fn outline_panel(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> Entity<OutlinePanel> {
    workspace.update_in(cx, |workspace, _window, cx| {
        workspace
            .panel::<OutlinePanel>(cx)
            .expect("no outline panel")
    })
}

fn display_entries(
    project: &Entity<Project>,
    multi_buffer_snapshot: &MultiBufferSnapshot,
    cached_entries: &[CachedEntry],
    selected_entry: Option<&PanelEntry>,
    cx: &mut App,
) -> String {
    let project = project.read(cx);
    let mut display_string = String::new();
    for entry in cached_entries {
        if !display_string.is_empty() {
            display_string += "\n";
        }
        for _ in 0..entry.depth {
            display_string += "  ";
        }
        display_string += &match &entry.entry {
            PanelEntry::Fs(entry) => match entry {
                FsEntry::ExternalFile(_) => {
                    panic!("Did not cover external files with tests")
                }
                FsEntry::Directory(directory) => {
                    let path = if let Some(worktree) = project
                        .worktree_for_id(directory.worktree_id, cx)
                        .filter(|worktree| {
                            worktree.read(cx).root_entry() == Some(&directory.entry.entry)
                        }) {
                        worktree
                            .read(cx)
                            .root_name()
                            .join(&directory.entry.path)
                            .as_unix_str()
                            .to_string()
                    } else {
                        directory
                            .entry
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string()
                    };
                    format!("{path}/")
                }
                FsEntry::File(file) => file
                    .entry
                    .path
                    .file_name()
                    .map(|name| name.to_string())
                    .unwrap_or_default(),
            },
            PanelEntry::FoldedDirs(folded_dirs) => folded_dirs
                .entries
                .iter()
                .filter_map(|dir| dir.path.file_name())
                .map(|name| name.to_string() + "/")
                .collect(),
            PanelEntry::Outline(outline_entry) => match outline_entry {
                OutlineEntry::Excerpt(_) => continue,
                OutlineEntry::Outline(outline_entry) => {
                    format!("outline: {}", outline_entry.text)
                }
            },
            PanelEntry::Search(search_entry) => {
                let search_data = search_entry.render_data.get_or_init(|| {
                    SearchData::new(&search_entry.match_range, multi_buffer_snapshot)
                });
                let mut search_result = String::new();
                let mut last_end = 0;
                for range in &search_data.search_match_indices {
                    search_result.push_str(&search_data.context_text[last_end..range.start]);
                    search_result.push('«');
                    search_result.push_str(&search_data.context_text[range.start..range.end]);
                    search_result.push('»');
                    last_end = range.end;
                }
                search_result.push_str(&search_data.context_text[last_end..]);

                format!("search: {search_result}")
            }
        };

        if Some(&entry.entry) == selected_entry {
            display_string += SELECTED_MARKER;
        }
    }
    display_string
}

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings = SettingsStore::test(cx);
        cx.set_global(settings);

        theme_settings::init(theme::LoadThemes::JustBase, cx);

        editor::init(cx);
        project_search::init(cx);
        buffer_search::init(cx);
        super::init(cx);
    });
}

// Based on https://github.com/rust-lang/rust-analyzer/
async fn populate_with_test_ra_project(fs: &FakeFs, root: &str) {
    fs.insert_tree(
        root,
        json!({
                "crates": {
                    "ide": {
                        "src": {
                            "inlay_hints": {
                                "fn_lifetime_fn.rs": r##"
    pub(super) fn hints(
        acc: &mut Vec<InlayHint>,
        config: &InlayHintsConfig,
        func: ast::Fn,
    ) -> Option<()> {
        // ... snip

        let mut used_names: FxHashMap<SmolStr, usize> =
            match config.param_names_for_lifetime_elision_hints {
                true => generic_param_list
                    .iter()
                    .flat_map(|gpl| gpl.lifetime_params())
                    .filter_map(|param| param.lifetime())
                    .filter_map(|lt| Some((SmolStr::from(lt.text().as_str().get(1..)?), 0)))
                    .collect(),
                false => Default::default(),
            };
        {
            let mut potential_lt_refs = potential_lt_refs.iter().filter(|&&(.., is_elided)| is_elided);
            if self_param.is_some() && potential_lt_refs.next().is_some() {
                allocated_lifetimes.push(if config.param_names_for_lifetime_elision_hints {
                    // self can't be used as a lifetime, so no need to check for collisions
                    "'self".into()
                } else {
                    gen_idx_name()
                });
            }
            potential_lt_refs.for_each(|(name, ..)| {
                let name = match name {
                    Some(it) if config.param_names_for_lifetime_elision_hints => {
                        if let Some(c) = used_names.get_mut(it.text().as_str()) {
                            *c += 1;
                            SmolStr::from(format!("'{text}{c}", text = it.text().as_str()))
                        } else {
                            used_names.insert(it.text().as_str().into(), 0);
                            SmolStr::from_iter(["\'", it.text().as_str()])
                        }
                    }
                    _ => gen_idx_name(),
                };
                allocated_lifetimes.push(name);
            });
        }

        // ... snip
    }

    // ... snip

        #[test]
        fn hints_lifetimes_named() {
            check_with_config(
                InlayHintsConfig { param_names_for_lifetime_elision_hints: true, ..TEST_CONFIG },
                r#"
    fn nested_in<'named>(named: &        &X<      &()>) {}
    //          ^'named1, 'named2, 'named3, $
                              //^'named1 ^'named2 ^'named3
    "#,
            );
        }

    // ... snip
    "##,
                            },
                    "inlay_hints.rs": r#"
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHintsConfig {
    // ... snip
    pub param_names_for_lifetime_elision_hints: bool,
    pub max_length: Option<usize>,
    // ... snip
}

impl Config {
    pub fn inlay_hints(&self) -> InlayHintsConfig {
        InlayHintsConfig {
            // ... snip
            param_names_for_lifetime_elision_hints: self
                .inlayHints_lifetimeElisionHints_useParameterNames()
                .to_owned(),
            max_length: self.inlayHints_maxLength().to_owned(),
            // ... snip
        }
    }
}
"#,
                    "static_index.rs": r#"
// ... snip
    fn add_file(&mut self, file_id: FileId) {
        let current_crate = crates_for(self.db, file_id).pop().map(Into::into);
        let folds = self.analysis.folding_ranges(file_id).unwrap();
        let inlay_hints = self
            .analysis
            .inlay_hints(
                &InlayHintsConfig {
                    // ... snip
                    closure_style: hir::ClosureStyle::ImplFn,
                    param_names_for_lifetime_elision_hints: false,
                    binding_mode_hints: false,
                    max_length: Some(25),
                    closure_capture_hints: false,
                    // ... snip
                },
                file_id,
                None,
            )
            .unwrap();
        // ... snip
}
// ... snip
"#
                        }
                    },
                    "rust-analyzer": {
                        "src": {
                            "cli": {
                                "analysis_stats.rs": r#"
    // ... snip
            for &file_id in &file_ids {
                _ = analysis.inlay_hints(
                    &InlayHintsConfig {
                        // ... snip
                        implicit_drop_hints: true,
                        lifetime_elision_hints: ide::LifetimeElisionHints::Always,
                        param_names_for_lifetime_elision_hints: true,
                        hide_named_constructor_hints: false,
                        hide_closure_initialization_hints: false,
                        closure_style: hir::ClosureStyle::ImplFn,
                        max_length: Some(25),
                        closing_brace_hints_min_lines: Some(20),
                        fields_to_resolve: InlayFieldsToResolve::empty(),
                        range_exclusive_hints: true,
                    },
                    file_id.into(),
                    None,
                );
            }
    // ... snip
                                "#,
                            },
                            "config.rs": r#"
            config_data! {
                /// Configs that only make sense when they are set by a client. As such they can only be defined
                /// by setting them using client's settings (e.g `settings.json` on VS Code).
                client: struct ClientDefaultConfigData <- ClientConfigInput -> {
                    // ... snip
                    /// Maximum length for inlay hints. Set to null to have an unlimited length.
                    inlayHints_maxLength: Option<usize>                        = Some(25),
                    // ... snip
                    /// Whether to prefer using parameter names as the name for elided lifetime hints if possible.
                    inlayHints_lifetimeElisionHints_useParameterNames: bool    = false,
                    // ... snip
                }
            }

            impl Config {
                // ... snip
                pub fn inlay_hints(&self) -> InlayHintsConfig {
                    InlayHintsConfig {
                        // ... snip
                        param_names_for_lifetime_elision_hints: self
                            .inlayHints_lifetimeElisionHints_useParameterNames()
                            .to_owned(),
                        max_length: self.inlayHints_maxLength().to_owned(),
                        // ... snip
                    }
                }
                // ... snip
            }
            "#
                            }
                    }
                }
        }),
    )
    .await;
}

fn snapshot(outline_panel: &OutlinePanel, cx: &App) -> MultiBufferSnapshot {
    outline_panel
        .active_editor()
        .unwrap()
        .read(cx)
        .buffer()
        .read(cx)
        .snapshot(cx)
}

fn selected_row_text(editor: &Entity<Editor>, cx: &mut App) -> String {
    editor.update(cx, |editor, cx| {
        let selections = editor
            .selections
            .all::<language::Point>(&editor.display_snapshot(cx));
        assert_eq!(
            selections.len(),
            1,
            "Active editor should have exactly one selection after any outline panel interactions"
        );
        let selection = selections.first().unwrap();
        let multi_buffer_snapshot = editor.buffer().read(cx).snapshot(cx);
        let line_start = language::Point::new(selection.start.row, 0);
        let line_end = multi_buffer_snapshot.clip_point(
            language::Point::new(selection.end.row, u32::MAX),
            language::Bias::Right,
        );
        multi_buffer_snapshot
            .text_for_range(line_start..line_end)
            .collect::<String>()
            .trim()
            .to_owned()
    })
}

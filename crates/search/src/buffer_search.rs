mod registrar;

use crate::{
    FocusSearch, NextHistoryQuery, PreviousHistoryQuery, ReplaceAll, ReplaceNext, SearchOption,
    SearchOptions, SearchSource, SelectAllMatches, SelectNextMatch, SelectPreviousMatch,
    ToggleCaseSensitive, ToggleRegex, ToggleReplace, ToggleSelection, ToggleWholeWord,
    buffer_search::registrar::WithResultsOrExternalQuery,
    search_bar::{
        ActionButtonState, HistoryNavigationDirection, alignment_element,
        filter_search_results_input, input_base_styles, render_action_button, render_text_input,
        should_navigate_history,
    },
};
use any_vec::AnyVec;
use collections::HashMap;
use editor::{
    Editor, EditorSettings, MultiBufferOffset, SplittableEditor, ToggleSplitDiff,
    actions::{Backtab, FoldAll, Tab, ToggleFoldAll, UnfoldAll},
    scroll::Autoscroll,
};
use futures::channel::oneshot;
use gpui::{
    Action as _, App, ClickEvent, Context, Entity, EventEmitter, Focusable,
    InteractiveElement as _, IntoElement, KeyContext, ParentElement as _, Render, ScrollHandle,
    Styled, Subscription, Task, TaskExt, WeakEntity, Window, div,
};
use language::{Language, LanguageRegistry};
use project::{
    search::SearchQuery,
    search_history::{SearchHistory, SearchHistoryCursor},
};

use fs::Fs;
use mav_actions::{
    OpenSettingsAt, outline::ToggleOutline, workspace::CopyPath, workspace::CopyRelativePath,
};
use settings::{DiffViewStyle, SeedQuerySetting, Settings, update_settings_file};
use std::{any::TypeId, sync::Arc};

use ui::{
    BASE_REM_SIZE_IN_PX, IconButtonShape, PlatformStyle, TextSize, Tooltip, prelude::*,
    render_modifiers, utils::SearchInputWidth,
};
use util::{ResultExt, paths::PathMatcher};
use workspace::{
    ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, Workspace,
    item::{ItemBufferKind, ItemHandle},
    searchable::{
        Direction, FilteredSearchRange, SearchEvent, SearchToken, SearchableItemHandle,
        WeakSearchableItemHandle,
    },
};

pub use registrar::{DivRegistrar, register_pane_search_actions};
use registrar::{ForDeployed, ForDismissed, SearchActionsRegistrar};

const MAX_BUFFER_SEARCH_HISTORY_SIZE: usize = 50;

pub use mav_actions::buffer_search::{
    Deploy, DeployReplace, Dismiss, FocusEditor, UseSelectionForFind,
};

pub enum Event {
    UpdateLocation,
    Dismissed,
}

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| BufferSearchBar::register(workspace))
        .detach();
}

pub struct BufferSearchBar {
    query_editor: Entity<Editor>,
    query_editor_focused: bool,
    replacement_editor: Entity<Editor>,
    replacement_editor_focused: bool,
    active_searchable_item: Option<Box<dyn SearchableItemHandle>>,
    active_match_index: Option<usize>,
    #[cfg(target_os = "macos")]
    active_searchable_item_subscriptions: Option<[Subscription; 2]>,
    #[cfg(not(target_os = "macos"))]
    active_searchable_item_subscriptions: Option<Subscription>,
    #[cfg(target_os = "macos")]
    pending_external_query: Option<(String, SearchOptions)>,
    active_search: Option<Arc<SearchQuery>>,
    searchable_items_with_matches:
        HashMap<Box<dyn WeakSearchableItemHandle>, (AnyVec<dyn Send>, SearchToken)>,
    pending_search: Option<Task<()>>,
    search_options: SearchOptions,
    default_options: SearchOptions,
    configured_options: SearchOptions,
    query_error: Option<String>,
    dismissed: bool,
    search_history: SearchHistory,
    search_history_cursor: SearchHistoryCursor,
    replace_enabled: bool,
    selection_search_enabled: Option<FilteredSearchRange>,
    scroll_handle: ScrollHandle,
    regex_language: Option<Arc<Language>>,
    splittable_editor: Option<WeakEntity<SplittableEditor>>,
    _splittable_editor_subscription: Option<Subscription>,
}

impl EventEmitter<Event> for BufferSearchBar {}
impl EventEmitter<workspace::ToolbarItemEvent> for BufferSearchBar {}
impl Render for BufferSearchBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle(cx);

        let has_splittable_editor = self.splittable_editor.is_some();
        let split_buttons = if has_splittable_editor {
            self.splittable_editor
                .as_ref()
                .and_then(|weak| weak.upgrade())
                .map(|splittable_editor| {
                    let editor_ref = splittable_editor.read(cx);
                    let diff_view_style = editor_ref.diff_view_style();

                    let is_split_set = diff_view_style == DiffViewStyle::Split;
                    let is_split_active = editor_ref.is_split();
                    let min_columns =
                        EditorSettings::get_global(cx).minimum_split_diff_width as u32;

                    let split_icon = if is_split_set && !is_split_active {
                        IconName::DiffSplitAuto
                    } else {
                        IconName::DiffSplit
                    };

                    h_flex()
                        .gap_1()
                        .child(
                            IconButton::new("diff-unified", IconName::DiffUnified)
                                .icon_size(IconSize::Small)
                                .toggle_state(diff_view_style == DiffViewStyle::Unified)
                                .tooltip(Tooltip::text("Unified"))
                                .on_click({
                                    let splittable_editor = splittable_editor.downgrade();
                                    move |_, window, cx| {
                                        update_settings_file(
                                            <dyn Fs>::global(cx),
                                            cx,
                                            |settings, _| {
                                                settings.editor.diff_view_style =
                                                    Some(DiffViewStyle::Unified);
                                            },
                                        );
                                        if diff_view_style == DiffViewStyle::Split {
                                            splittable_editor
                                                .update(cx, |editor, cx| {
                                                    editor.toggle_split(
                                                        &ToggleSplitDiff,
                                                        window,
                                                        cx,
                                                    );
                                                })
                                                .ok();
                                        }
                                    }
                                }),
                        )
                        .child(
                            IconButton::new("diff-split", split_icon)
                                .toggle_state(diff_view_style == DiffViewStyle::Split)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::element(move |_, cx| {
                                    let message = if is_split_set && !is_split_active {
                                        format!("Split when wider than {} columns", min_columns)
                                            .into()
                                    } else {
                                        SharedString::from("Split")
                                    };

                                    v_flex()
                                        .child(message)
                                        .child(
                                            h_flex()
                                                .gap_0p5()
                                                .text_ui_sm(cx)
                                                .text_color(Color::Muted.color(cx))
                                                .children(render_modifiers(
                                                    &gpui::Modifiers::secondary_key(),
                                                    PlatformStyle::platform(),
                                                    None,
                                                    Some(TextSize::Small.rems(cx).into()),
                                                    false,
                                                ))
                                                .child("click to change min width"),
                                        )
                                        .into_any()
                                }))
                                .on_click({
                                    let splittable_editor = splittable_editor.downgrade();
                                    move |_, window, cx| {
                                        if window.modifiers().secondary() {
                                            window.dispatch_action(
                                                OpenSettingsAt {
                                                    path: "minimum_split_diff_width".to_string(),
                                                    target: None,
                                                }
                                                .boxed_clone(),
                                                cx,
                                            );
                                        } else {
                                            update_settings_file(
                                                <dyn Fs>::global(cx),
                                                cx,
                                                |settings, _| {
                                                    settings.editor.diff_view_style =
                                                        Some(DiffViewStyle::Split);
                                                },
                                            );
                                            if diff_view_style == DiffViewStyle::Unified {
                                                splittable_editor
                                                    .update(cx, |editor, cx| {
                                                        editor.toggle_split(
                                                            &ToggleSplitDiff,
                                                            window,
                                                            cx,
                                                        );
                                                    })
                                                    .ok();
                                            }
                                        }
                                    }
                                }),
                        )
                })
        } else {
            None
        };

        let collapse_expand_button = if self.needs_expand_collapse_option(cx) {
            let query_editor_focus = self.query_editor.focus_handle(cx);

            let is_collapsed = self
                .active_searchable_item
                .as_ref()
                .and_then(|item| item.act_as_type(TypeId::of::<Editor>(), cx))
                .and_then(|item| item.downcast::<Editor>().ok())
                .map(|editor: Entity<Editor>| editor.read(cx).has_any_buffer_folded(cx))
                .unwrap_or_default();
            let (icon, tooltip_label) = if is_collapsed {
                (IconName::ChevronUpDown, "Expand All Files")
            } else {
                (IconName::ChevronDownUp, "Collapse All Files")
            };

            let collapse_expand_icon_button = |id| {
                IconButton::new(id, icon)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_, cx| {
                        Tooltip::for_action_in(
                            tooltip_label,
                            &ToggleFoldAll,
                            &query_editor_focus,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.toggle_fold_all(&ToggleFoldAll, window, cx);
                    }))
            };

            if self.dismissed {
                return h_flex()
                    .pl_0p5()
                    .gap_1()
                    .child(collapse_expand_icon_button(
                        "multibuffer-collapse-expand-empty",
                    ))
                    .when(has_splittable_editor, |this| this.children(split_buttons))
                    .into_any_element();
            }

            Some(
                h_flex()
                    .gap_1()
                    .child(collapse_expand_icon_button("multibuffer-collapse-expand"))
                    .children(split_buttons)
                    .into_any_element(),
            )
        } else {
            None
        };

        let narrow_mode =
            self.scroll_handle.bounds().size.width / window.rem_size() < 340. / BASE_REM_SIZE_IN_PX;

        let workspace::searchable::SearchOptions {
            case,
            word,
            regex,
            replacement,
            selection,
            select_all,
            find_in_results,
        } = self.supported_options(cx);

        self.query_editor.update(cx, |query_editor, cx| {
            if query_editor.placeholder_text(cx).is_none() {
                query_editor.set_placeholder_text("Search…", window, cx);
            }
        });

        self.replacement_editor.update(cx, |editor, cx| {
            editor.set_placeholder_text("Replace with…", window, cx);
        });

        let mut color_override = None;
        let match_text = self
            .active_searchable_item
            .as_ref()
            .and_then(|searchable_item| {
                if self.query(cx).is_empty() {
                    return None;
                }
                let matches_count = self
                    .searchable_items_with_matches
                    .get(&searchable_item.downgrade())
                    .map(|(matches, _)| matches.len())
                    .unwrap_or(0);
                if let Some(match_ix) = self.active_match_index {
                    Some(format!("{}/{}", match_ix + 1, matches_count))
                } else {
                    color_override = Some(Color::Error); // No matches found
                    None
                }
            })
            .unwrap_or_else(|| "0/0".to_string());
        let should_show_replace_input = self.replace_enabled && replacement;
        let in_replace = self.replacement_editor.focus_handle(cx).is_focused(window);

        let theme_colors = cx.theme().colors();
        let query_border = if self.query_error.is_some() {
            Color::Error.color(cx)
        } else {
            theme_colors.border
        };
        let replacement_border = theme_colors.border;

        let container_width = window.viewport_size().width;
        let input_width = SearchInputWidth::calc_width(container_width);

        let input_base_styles =
            |border_color| input_base_styles(border_color, |div| div.w(input_width));

        let input_style = if find_in_results {
            filter_search_results_input(query_border, |div| div.w(input_width), cx)
        } else {
            input_base_styles(query_border)
        };

        let query_column = input_style
            .child(div().flex_1().min_w_0().py_1().child(render_text_input(
                &self.query_editor,
                color_override,
                cx,
            )))
            .child(
                h_flex()
                    .flex_none()
                    .gap_1()
                    .when(case, |div| {
                        div.child(SearchOption::CaseSensitive.as_button(
                            self.search_options,
                            SearchSource::Buffer,
                            focus_handle.clone(),
                        ))
                    })
                    .when(word, |div| {
                        div.child(SearchOption::WholeWord.as_button(
                            self.search_options,
                            SearchSource::Buffer,
                            focus_handle.clone(),
                        ))
                    })
                    .when(regex, |div| {
                        div.child(SearchOption::Regex.as_button(
                            self.search_options,
                            SearchSource::Buffer,
                            focus_handle.clone(),
                        ))
                    }),
            );

        let mode_column = h_flex()
            .gap_1()
            .min_w_64()
            .when(replacement, |this| {
                this.child(render_action_button(
                    "buffer-search-bar-toggle",
                    IconName::Replace,
                    self.replace_enabled.then_some(ActionButtonState::Toggled),
                    "Toggle Replace",
                    &ToggleReplace,
                    focus_handle.clone(),
                ))
            })
            .when(selection, |this| {
                this.child(
                    IconButton::new(
                        "buffer-search-bar-toggle-search-selection-button",
                        IconName::Quote,
                    )
                    .style(ButtonStyle::Subtle)
                    .shape(IconButtonShape::Square)
                    .when(self.selection_search_enabled.is_some(), |button| {
                        button.style(ButtonStyle::Filled)
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.toggle_selection(&ToggleSelection, window, cx);
                    }))
                    .toggle_state(self.selection_search_enabled.is_some())
                    .tooltip({
                        let focus_handle = focus_handle.clone();
                        move |_window, cx| {
                            Tooltip::for_action_in(
                                "Toggle Search Selection",
                                &ToggleSelection,
                                &focus_handle,
                                cx,
                            )
                        }
                    }),
                )
            })
            .when(!find_in_results, |el| {
                let query_focus = self.query_editor.focus_handle(cx);
                let matches_column = h_flex()
                    .pl_2()
                    .ml_2()
                    .border_l_1()
                    .border_color(theme_colors.border_variant)
                    .child(render_action_button(
                        "buffer-search-nav-button",
                        ui::IconName::ChevronLeft,
                        self.active_match_index
                            .is_none()
                            .then_some(ActionButtonState::Disabled),
                        "Select Previous Match",
                        &SelectPreviousMatch,
                        query_focus.clone(),
                    ))
                    .child(render_action_button(
                        "buffer-search-nav-button",
                        ui::IconName::ChevronRight,
                        self.active_match_index
                            .is_none()
                            .then_some(ActionButtonState::Disabled),
                        "Select Next Match",
                        &SelectNextMatch,
                        query_focus.clone(),
                    ))
                    .when(!narrow_mode, |this| {
                        this.child(div().ml_2().min_w(rems_from_px(40.)).child(
                            Label::new(match_text).size(LabelSize::Small).color(
                                if self.active_match_index.is_some() {
                                    Color::Default
                                } else {
                                    Color::Disabled
                                },
                            ),
                        ))
                    });

                el.when(select_all, |el| {
                    el.child(render_action_button(
                        "buffer-search-nav-button",
                        IconName::SelectAll,
                        Default::default(),
                        "Select All Matches",
                        &SelectAllMatches,
                        query_focus.clone(),
                    ))
                })
                .child(matches_column)
            })
            .when(find_in_results, |el| {
                el.child(render_action_button(
                    "buffer-search",
                    IconName::Close,
                    Default::default(),
                    "Close Search Bar",
                    &Dismiss,
                    focus_handle.clone(),
                ))
            });

        let has_collapse_button = collapse_expand_button.is_some();

        let search_line = h_flex()
            .w_full()
            .gap_2()
            .when(find_in_results, |el| el.child(alignment_element()))
            .when(!find_in_results && has_collapse_button, |el| {
                el.pl_0p5().child(collapse_expand_button.expect("button"))
            })
            .child(query_column)
            .child(mode_column);

        let replace_line = should_show_replace_input.then(|| {
            let replace_column = input_base_styles(replacement_border).child(
                div()
                    .flex_1()
                    .py_1()
                    .child(render_text_input(&self.replacement_editor, None, cx)),
            );
            let focus_handle = self.replacement_editor.read(cx).focus_handle(cx);

            let replace_actions = h_flex()
                .min_w_64()
                .gap_1()
                .child(render_action_button(
                    "buffer-search-replace-button",
                    IconName::ReplaceNext,
                    Default::default(),
                    "Replace Next Match",
                    &ReplaceNext,
                    focus_handle.clone(),
                ))
                .child(render_action_button(
                    "buffer-search-replace-button",
                    IconName::ReplaceAll,
                    Default::default(),
                    "Replace All Matches",
                    &ReplaceAll,
                    focus_handle,
                ));

            h_flex()
                .w_full()
                .gap_2()
                .when(has_collapse_button, |this| this.child(alignment_element()))
                .child(replace_column)
                .child(replace_actions)
        });

        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("BufferSearchBar");
        if in_replace {
            key_context.add("in_replace");
        }

        let query_error_line = self.query_error.as_ref().map(|error| {
            Label::new(error)
                .size(LabelSize::Small)
                .color(Color::Error)
                .mt_neg_1()
                .ml_2()
        });

        let search_line =
            h_flex()
                .relative()
                .child(search_line)
                .when(!narrow_mode && !find_in_results, |this| {
                    this.child(
                        h_flex()
                            .absolute()
                            .right_0()
                            .when(has_collapse_button, |this| {
                                this.pr_2()
                                    .border_r_1()
                                    .border_color(cx.theme().colors().border_variant)
                            })
                            .child(render_action_button(
                                "buffer-search",
                                IconName::Close,
                                Default::default(),
                                "Close Search Bar",
                                &Dismiss,
                                focus_handle.clone(),
                            )),
                    )
                });

        v_flex()
            .id("buffer_search")
            .gap_2()
            .w_full()
            .track_scroll(&self.scroll_handle)
            .key_context(key_context)
            .capture_action(cx.listener(Self::tab))
            .capture_action(cx.listener(Self::backtab))
            .capture_action(cx.listener(Self::toggle_fold_all))
            .on_action(cx.listener(Self::previous_history_query))
            .on_action(cx.listener(Self::next_history_query))
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::select_next_match))
            .on_action(cx.listener(Self::select_prev_match))
            .on_action(cx.listener(|this, _: &ToggleOutline, window, cx| {
                if let Some(active_searchable_item) = &mut this.active_searchable_item {
                    active_searchable_item.relay_action(Box::new(ToggleOutline), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CopyPath, window, cx| {
                if let Some(active_searchable_item) = &mut this.active_searchable_item {
                    active_searchable_item.relay_action(Box::new(CopyPath), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &CopyRelativePath, window, cx| {
                if let Some(active_searchable_item) = &mut this.active_searchable_item {
                    active_searchable_item.relay_action(Box::new(CopyRelativePath), window, cx);
                }
            }))
            .when(replacement, |this| {
                this.on_action(cx.listener(Self::toggle_replace))
                    .on_action(cx.listener(Self::replace_next))
                    .on_action(cx.listener(Self::replace_all))
            })
            .when(case, |this| {
                this.on_action(cx.listener(Self::toggle_case_sensitive))
            })
            .when(word, |this| {
                this.on_action(cx.listener(Self::toggle_whole_word))
            })
            .when(regex, |this| {
                this.on_action(cx.listener(Self::toggle_regex))
            })
            .when(selection, |this| {
                this.on_action(cx.listener(Self::toggle_selection))
            })
            .child(search_line)
            .children(query_error_line)
            .children(replace_line)
            .into_any_element()
    }
}

impl Focusable for BufferSearchBar {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.query_editor.focus_handle(cx)
    }
}

impl ToolbarItemView for BufferSearchBar {
    fn contribute_context(&self, context: &mut KeyContext, _cx: &App) {
        if !self.dismissed {
            context.add("buffer_search_deployed");
        }
    }

    fn set_active_pane_item(
        &mut self,
        item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        cx.notify();
        self.active_searchable_item_subscriptions.take();
        self.active_searchable_item.take();
        self.splittable_editor = None;
        self._splittable_editor_subscription = None;

        self.pending_search.take();

        if let Some(splittable_editor) = item
            .and_then(|item| item.act_as_type(TypeId::of::<SplittableEditor>(), cx))
            .and_then(|entity| entity.downcast::<SplittableEditor>().ok())
        {
            self._splittable_editor_subscription =
                Some(cx.observe(&splittable_editor, |_, _, cx| cx.notify()));
            self.splittable_editor = Some(splittable_editor.downgrade());
        }

        if let Some(searchable_item_handle) =
            item.and_then(|item| item.to_searchable_item_handle(cx))
        {
            let this = cx.entity().downgrade();

            let search_event_subscription = searchable_item_handle.subscribe_to_search_events(
                window,
                cx,
                Box::new(move |search_event, window, cx| {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.on_active_searchable_item_event(search_event, window, cx)
                        });
                    }
                }),
            );

            #[cfg(target_os = "macos")]
            {
                let item_focus_handle = searchable_item_handle.item_focus_handle(cx);

                self.active_searchable_item_subscriptions = Some([
                    search_event_subscription,
                    cx.on_focus(&item_focus_handle, window, |this, window, cx| {
                        if this.query_editor_focused || this.replacement_editor_focused {
                            // no need to read pasteboard since focus came from toolbar
                            return;
                        }

                        cx.defer_in(window, |this, window, cx| {
                            let Some(item) = cx.read_from_find_pasteboard() else {
                                return;
                            };
                            let Some(text) = item.text() else {
                                return;
                            };

                            if this.query(cx) == text {
                                return;
                            }

                            let search_options = item
                                .metadata()
                                .and_then(|m| m.parse().ok())
                                .and_then(SearchOptions::from_bits)
                                .unwrap_or(this.search_options);

                            if this.dismissed {
                                this.pending_external_query = Some((text, search_options));
                            } else {
                                drop(this.search(&text, Some(search_options), true, window, cx));
                            }
                        });
                    }),
                ]);
            }
            #[cfg(not(target_os = "macos"))]
            {
                self.active_searchable_item_subscriptions = Some(search_event_subscription);
            }

            let is_project_search = searchable_item_handle.supported_options(cx).find_in_results;
            self.active_searchable_item = Some(searchable_item_handle);
            drop(self.update_matches(true, false, window, cx));
            if self.needs_expand_collapse_option(cx) {
                return ToolbarItemLocation::PrimaryLeft;
            } else if !self.is_dismissed() {
                if is_project_search {
                    self.dismiss(&Default::default(), window, cx);
                } else {
                    return ToolbarItemLocation::Secondary;
                }
            }
        }
        ToolbarItemLocation::Hidden
    }
}

impl BufferSearchBar {
    pub fn query_editor_focused(&self) -> bool {
        self.query_editor_focused
    }

    pub fn register(registrar: &mut impl SearchActionsRegistrar) {
        registrar.register_handler(ForDeployed(|this, _: &FocusSearch, window, cx| {
            this.query_editor.focus_handle(cx).focus(window, cx);
            this.select_query(window, cx);
        }));
        registrar.register_handler(ForDeployed(
            |this, action: &ToggleCaseSensitive, window, cx| {
                if this.supported_options(cx).case {
                    this.toggle_case_sensitive(action, window, cx);
                }
            },
        ));
        registrar.register_handler(ForDeployed(|this, action: &ToggleWholeWord, window, cx| {
            if this.supported_options(cx).word {
                this.toggle_whole_word(action, window, cx);
            }
        }));
        registrar.register_handler(ForDeployed(|this, action: &ToggleRegex, window, cx| {
            if this.supported_options(cx).regex {
                this.toggle_regex(action, window, cx);
            }
        }));
        registrar.register_handler(ForDeployed(|this, action: &ToggleSelection, window, cx| {
            if this.supported_options(cx).selection {
                this.toggle_selection(action, window, cx);
            } else {
                cx.propagate();
            }
        }));
        registrar.register_handler(ForDeployed(|this, action: &ToggleReplace, window, cx| {
            if this.supported_options(cx).replacement {
                this.toggle_replace(action, window, cx);
            } else {
                cx.propagate();
            }
        }));
        registrar.register_handler(WithResultsOrExternalQuery(
            |this, action: &SelectNextMatch, window, cx| {
                if this.supported_options(cx).find_in_results {
                    cx.propagate();
                } else {
                    this.select_next_match(action, window, cx);
                }
            },
        ));
        registrar.register_handler(WithResultsOrExternalQuery(
            |this, action: &SelectPreviousMatch, window, cx| {
                if this.supported_options(cx).find_in_results {
                    cx.propagate();
                } else {
                    this.select_prev_match(action, window, cx);
                }
            },
        ));
        registrar.register_handler(WithResultsOrExternalQuery(
            |this, action: &SelectAllMatches, window, cx| {
                if this.supported_options(cx).find_in_results {
                    cx.propagate();
                } else {
                    this.select_all_matches(action, window, cx);
                }
            },
        ));
        registrar.register_handler(ForDeployed(
            |this, _: &editor::actions::Cancel, window, cx| {
                this.dismiss(&Dismiss, window, cx);
            },
        ));
        registrar.register_handler(ForDeployed(|this, _: &Dismiss, window, cx| {
            this.dismiss(&Dismiss, window, cx);
        }));

        // register deploy buffer search for both search bar states, since we want to focus into the search bar
        // when the deploy action is triggered in the buffer.
        registrar.register_handler(ForDeployed(|this, deploy, window, cx| {
            this.deploy(deploy, None, window, cx);
        }));
        registrar.register_handler(ForDismissed(|this, deploy, window, cx| {
            this.deploy(deploy, None, window, cx);
        }));
        registrar.register_handler(ForDeployed(|this, _: &DeployReplace, window, cx| {
            if this.supported_options(cx).find_in_results {
                cx.propagate();
            } else {
                this.deploy(&Deploy::replace(), None, window, cx);
            }
        }));
        registrar.register_handler(ForDismissed(|this, _: &DeployReplace, window, cx| {
            if this.supported_options(cx).find_in_results {
                cx.propagate();
            } else {
                this.deploy(&Deploy::replace(), None, window, cx);
            }
        }));
        registrar.register_handler(ForDeployed(
            |this, action: &UseSelectionForFind, window, cx| {
                this.use_selection_for_find(action, window, cx);
            },
        ));
        registrar.register_handler(ForDismissed(
            |this, action: &UseSelectionForFind, window, cx| {
                this.use_selection_for_find(action, window, cx);
            },
        ));
    }

    pub fn new(
        languages: Option<Arc<LanguageRegistry>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query_editor = cx.new(|cx| {
            let mut editor = Editor::auto_height(1, 4, window, cx);
            editor.set_use_autoclose(false);
            editor.set_use_selection_highlight(false);
            editor
        });
        cx.subscribe_in(&query_editor, window, Self::on_query_editor_event)
            .detach();
        let replacement_editor = cx.new(|cx| Editor::auto_height(1, 4, window, cx));
        cx.subscribe(&replacement_editor, Self::on_replacement_editor_event)
            .detach();

        let search_options = SearchOptions::from_settings(&EditorSettings::get_global(cx).search);
        if let Some(languages) = languages {
            let query_buffer = query_editor
                .read(cx)
                .buffer()
                .read(cx)
                .as_singleton()
                .expect("query editor should be backed by a singleton buffer");

            query_buffer
                .read(cx)
                .set_language_registry(languages.clone());

            cx.spawn(async move |buffer_search_bar, cx| {
                use anyhow::Context as _;

                let regex_language = languages
                    .language_for_name("regex")
                    .await
                    .context("loading regex language")?;

                buffer_search_bar
                    .update(cx, |buffer_search_bar, cx| {
                        buffer_search_bar.regex_language = Some(regex_language);
                        buffer_search_bar.adjust_query_regex_language(cx);
                    })
                    .ok();
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }

        Self {
            query_editor,
            query_editor_focused: false,
            replacement_editor,
            replacement_editor_focused: false,
            active_searchable_item: None,
            active_searchable_item_subscriptions: None,
            #[cfg(target_os = "macos")]
            pending_external_query: None,
            active_match_index: None,
            searchable_items_with_matches: Default::default(),
            default_options: search_options,
            configured_options: search_options,
            search_options,
            pending_search: None,
            query_error: None,
            dismissed: true,
            search_history: SearchHistory::new(
                Some(MAX_BUFFER_SEARCH_HISTORY_SIZE),
                project::search_history::QueryInsertionBehavior::ReplacePreviousIfContains,
            ),
            search_history_cursor: Default::default(),
            active_search: None,
            replace_enabled: false,
            selection_search_enabled: None,
            scroll_handle: ScrollHandle::new(),
            regex_language: None,
            splittable_editor: None,
            _splittable_editor_subscription: None,
        }
    }

    pub fn is_dismissed(&self) -> bool {
        self.dismissed
    }

    pub fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        self.dismissed = true;
        cx.emit(Event::Dismissed);
        self.query_error = None;
        self.sync_select_next_case_sensitivity(cx);

        for searchable_item in self.searchable_items_with_matches.keys() {
            if let Some(searchable_item) =
                WeakSearchableItemHandle::upgrade(searchable_item.as_ref(), cx)
            {
                searchable_item.clear_matches(window, cx);
            }
        }

        let needs_collapse_expand = self.needs_expand_collapse_option(cx);

        if let Some(active_editor) = self.active_searchable_item.as_mut() {
            self.selection_search_enabled = None;
            self.replace_enabled = false;
            active_editor.search_bar_visibility_changed(false, window, cx);
            active_editor.toggle_filtered_search_ranges(None, window, cx);
            let handle = active_editor.item_focus_handle(cx);
            self.focus(&handle, window, cx);
        }

        if needs_collapse_expand {
            cx.emit(Event::UpdateLocation);
            cx.emit(ToolbarItemEvent::ChangeLocation(
                ToolbarItemLocation::PrimaryLeft,
            ));
            cx.notify();
            return;
        }
        cx.emit(Event::UpdateLocation);
        cx.emit(ToolbarItemEvent::ChangeLocation(
            ToolbarItemLocation::Hidden,
        ));
        cx.notify();
    }

    pub fn deploy(
        &mut self,
        deploy: &Deploy,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let filtered_search_range = if deploy.selection_search_enabled {
            Some(FilteredSearchRange::Default)
        } else {
            None
        };
        if self.show(window, cx) {
            if let Some(active_item) = self.active_searchable_item.as_mut() {
                active_item.toggle_filtered_search_ranges(filtered_search_range, window, cx);
            }
            self.search_suggested(seed_query_override, window, cx);
            self.smartcase(window, cx);
            self.sync_select_next_case_sensitivity(cx);
            self.replace_enabled |= deploy.replace_enabled;
            self.selection_search_enabled =
                self.selection_search_enabled
                    .or(if deploy.selection_search_enabled {
                        Some(FilteredSearchRange::Default)
                    } else {
                        None
                    });
            if deploy.focus {
                let mut handle = self.query_editor.focus_handle(cx);
                let mut select_query = true;

                let has_seed_text = self
                    .query_suggestion(seed_query_override, window, cx)
                    .is_some();
                if deploy.replace_enabled && has_seed_text {
                    handle = self.replacement_editor.focus_handle(cx);
                    select_query = false;
                };

                if select_query {
                    self.select_query(window, cx);
                }

                window.focus(&handle, cx);
            }
            return true;
        }

        cx.propagate();
        false
    }

    pub fn toggle(&mut self, action: &Deploy, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_dismissed() {
            self.deploy(action, None, window, cx);
        } else {
            self.dismiss(&Dismiss, window, cx);
        }
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(handle) = self.active_searchable_item.as_ref() else {
            return false;
        };

        let configured_options =
            SearchOptions::from_settings(&EditorSettings::get_global(cx).search);
        let settings_changed = configured_options != self.configured_options;

        if self.dismissed && settings_changed {
            // Only update configuration options when search bar is dismissed,
            // so we don't miss updates even after calling show twice
            self.configured_options = configured_options;
            self.search_options = configured_options;
            self.default_options = configured_options;
        }

        // This isn't a normal setting; it's only applicable to vim search.
        self.search_options.remove(SearchOptions::BACKWARDS);

        self.dismissed = false;
        self.adjust_query_regex_language(cx);
        handle.search_bar_visibility_changed(true, window, cx);
        cx.notify();
        cx.emit(Event::UpdateLocation);
        cx.emit(ToolbarItemEvent::ChangeLocation(
            if self.needs_expand_collapse_option(cx) {
                ToolbarItemLocation::PrimaryLeft
            } else {
                ToolbarItemLocation::Secondary
            },
        ));
        true
    }

    fn supported_options(&self, cx: &mut Context<Self>) -> workspace::searchable::SearchOptions {
        self.active_searchable_item
            .as_ref()
            .map(|item| item.supported_options(cx))
            .unwrap_or_default()
    }

    // We provide an expand/collapse button if we are in a multibuffer
    // and not doing a project search.
    fn needs_expand_collapse_option(&self, cx: &App) -> bool {
        if let Some(item) = &self.active_searchable_item {
            let buffer_kind = item.buffer_kind(cx);

            if buffer_kind == ItemBufferKind::Singleton {
                return false;
            }

            let workspace::searchable::SearchOptions {
                find_in_results, ..
            } = item.supported_options(cx);
            !find_in_results
        } else {
            false
        }
    }

    fn toggle_fold_all(&mut self, _: &ToggleFoldAll, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_fold_all_in_item(window, cx);
    }

    fn toggle_fold_all_in_item(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = &self.active_searchable_item {
            if let Some(item) = item.act_as_type(TypeId::of::<Editor>(), cx) {
                let editor = item.downcast::<Editor>().expect("Is an editor");
                editor.update(cx, |editor, cx| {
                    let is_collapsed = editor.has_any_buffer_folded(cx);
                    if is_collapsed {
                        editor.unfold_all(&UnfoldAll, window, cx);
                    } else {
                        editor.fold_all(&FoldAll, window, cx);
                    }
                })
            }
        }
    }

    pub fn search_suggested(
        &mut self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search = self
            .query_suggestion(seed_query_override, window, cx)
            .map(|suggestion| {
                self.search(&suggestion, Some(self.default_options), true, window, cx)
            });

        #[cfg(target_os = "macos")]
        let search = search.or_else(|| {
            self.pending_external_query
                .take()
                .map(|(query, options)| self.search(&query, Some(options), true, window, cx))
        });

        if let Some(search) = search {
            cx.spawn_in(window, async move |this, cx| {
                if search.await.is_ok() {
                    this.update_in(cx, |this, window, cx| {
                        if !this.dismissed {
                            this.activate_current_match(window, cx)
                        }
                    })
                } else {
                    Ok(())
                }
            })
            .detach_and_log_err(cx);
        }
    }

    pub fn activate_current_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(match_ix) = self.active_match_index
            && let Some(active_searchable_item) = self.active_searchable_item.as_ref()
            && let Some((matches, token)) = self
                .searchable_items_with_matches
                .get(&active_searchable_item.downgrade())
        {
            active_searchable_item.activate_match(match_ix, matches, *token, window, cx)
        }
    }

    pub fn select_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query_editor.update(cx, |query_editor, cx| {
            query_editor.select_all(&Default::default(), window, cx);
        });
    }

    pub fn query(&self, cx: &App) -> String {
        self.query_editor.read(cx).text(cx)
    }

    pub fn replacement(&self, cx: &mut App) -> String {
        self.replacement_editor.read(cx).text(cx)
    }

    pub fn query_suggestion(
        &mut self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.active_searchable_item
            .as_ref()
            .map(|searchable_item| {
                searchable_item.query_suggestion(seed_query_override, window, cx)
            })
            .filter(|suggestion| !suggestion.is_empty())
    }

    pub fn set_replacement(&mut self, replacement: Option<&str>, cx: &mut Context<Self>) {
        if replacement.is_none() {
            self.replace_enabled = false;
            return;
        }
        self.replace_enabled = true;
        self.replacement_editor
            .update(cx, |replacement_editor, cx| {
                replacement_editor
                    .buffer()
                    .update(cx, |replacement_buffer, cx| {
                        let len = replacement_buffer.len(cx);
                        replacement_buffer.edit(
                            [(MultiBufferOffset(0)..len, replacement.unwrap())],
                            None,
                            cx,
                        );
                    });
            });
    }

    pub fn focus_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus(&self.replacement_editor.focus_handle(cx), window, cx);
        cx.notify();
    }

    pub fn search(
        &mut self,
        query: &str,
        options: Option<SearchOptions>,
        add_to_history: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<()> {
        let options = options.unwrap_or(self.default_options);
        let updated = query != self.query(cx) || self.search_options != options;
        if updated {
            self.query_editor.update(cx, |query_editor, cx| {
                query_editor.buffer().update(cx, |query_buffer, cx| {
                    let len = query_buffer.len(cx);
                    query_buffer.edit([(MultiBufferOffset(0)..len, query)], None, cx);
                });
                query_editor.request_autoscroll(Autoscroll::fit(), cx);
            });
            self.set_search_options(options, cx);
            self.clear_matches(window, cx);
            #[cfg(target_os = "macos")]
            self.update_find_pasteboard(cx);
            cx.notify();
        }
        self.update_matches(!updated, add_to_history, window, cx)
    }

    #[cfg(target_os = "macos")]
    pub fn update_find_pasteboard(&mut self, cx: &mut App) {
        cx.write_to_find_pasteboard(gpui::ClipboardItem::new_string_with_metadata(
            self.query(cx),
            self.search_options.bits().to_string(),
        ));
    }

    pub fn use_selection_for_find(
        &mut self,
        _: &UseSelectionForFind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.deploy(
            &Deploy {
                focus: false,
                replace_enabled: false,
                selection_search_enabled: false,
            },
            Some(SeedQuerySetting::Always),
            window,
            cx,
        );
    }

    pub fn focus_editor(&mut self, _: &FocusEditor, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_editor) = self.active_searchable_item.as_ref() {
            let handle = active_editor.item_focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    pub fn toggle_search_option(
        &mut self,
        search_option: SearchOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_options.toggle(search_option);
        self.default_options = self.search_options;
        drop(self.update_matches(false, false, window, cx));
        self.adjust_query_regex_language(cx);
        self.sync_select_next_case_sensitivity(cx);
        cx.notify();
    }

    pub fn has_search_option(&mut self, search_option: SearchOptions) -> bool {
        self.search_options.contains(search_option)
    }

    pub fn enable_search_option(
        &mut self,
        search_option: SearchOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.search_options.contains(search_option) {
            self.toggle_search_option(search_option, window, cx)
        }
    }

    pub fn set_search_within_selection(
        &mut self,
        search_within_selection: Option<FilteredSearchRange>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<oneshot::Receiver<()>> {
        let active_item = self.active_searchable_item.as_mut()?;
        self.selection_search_enabled = search_within_selection;
        active_item.toggle_filtered_search_ranges(self.selection_search_enabled, window, cx);
        cx.notify();
        Some(self.update_matches(false, false, window, cx))
    }

    pub fn set_search_options(&mut self, search_options: SearchOptions, cx: &mut Context<Self>) {
        self.search_options = search_options;
        self.adjust_query_regex_language(cx);
        self.sync_select_next_case_sensitivity(cx);
        cx.notify();
    }

    pub fn clear_search_within_ranges(
        &mut self,
        search_options: SearchOptions,
        cx: &mut Context<Self>,
    ) {
        self.search_options = search_options;
        self.adjust_query_regex_language(cx);
        cx.notify();
    }

    fn select_next_match(
        &mut self,
        _: &SelectNextMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_match(Direction::Next, 1, window, cx);
    }

    fn select_prev_match(
        &mut self,
        _: &SelectPreviousMatch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_match(Direction::Prev, 1, window, cx);
    }

    pub fn select_all_matches(
        &mut self,
        _: &SelectAllMatches,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dismissed
            && self.active_match_index.is_some()
            && let Some(searchable_item) = self.active_searchable_item.as_ref()
            && let Some((matches, token)) = self
                .searchable_items_with_matches
                .get(&searchable_item.downgrade())
        {
            searchable_item.select_matches(matches, *token, window, cx);
            self.focus_editor(&FocusEditor, window, cx);
        }
    }

    pub fn select_match(
        &mut self,
        direction: Direction,
        count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        if let Some((query, options)) = self.pending_external_query.take() {
            let search_rx = self.search(&query, Some(options), true, window, cx);
            cx.spawn_in(window, async move |this, cx| {
                if search_rx.await.is_ok() {
                    this.update_in(cx, |this, window, cx| {
                        this.activate_current_match(window, cx);
                    })
                    .ok();
                }
            })
            .detach();

            return;
        }

        if let Some(index) = self.active_match_index
            && let Some(searchable_item) = self.active_searchable_item.as_ref()
            && let Some((matches, token)) = self
                .searchable_items_with_matches
                .get(&searchable_item.downgrade())
                .filter(|(matches, _)| !matches.is_empty())
        {
            // If 'wrapscan' is disabled, searches do not wrap around the end of the file.
            if !EditorSettings::get_global(cx).search_wrap
                && ((direction == Direction::Next && index + count >= matches.len())
                    || (direction == Direction::Prev && index < count))
            {
                crate::show_no_more_matches(window, cx);
                return;
            }
            let new_match_index = searchable_item
                .match_index_for_direction(matches, index, direction, count, *token, window, cx);
            self.active_match_index = Some(new_match_index);

            searchable_item.update_matches(matches, Some(new_match_index), *token, window, cx);
            searchable_item.activate_match(new_match_index, matches, *token, window, cx);
        }
    }

    pub fn select_first_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(searchable_item) = self.active_searchable_item.as_ref()
            && let Some((matches, token)) = self
                .searchable_items_with_matches
                .get(&searchable_item.downgrade())
        {
            if matches.is_empty() {
                return;
            }
            searchable_item.update_matches(matches, Some(0), *token, window, cx);
            searchable_item.activate_match(0, matches, *token, window, cx);
        }
    }

    pub fn select_last_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(searchable_item) = self.active_searchable_item.as_ref()
            && let Some((matches, token)) = self
                .searchable_items_with_matches
                .get(&searchable_item.downgrade())
        {
            if matches.is_empty() {
                return;
            }
            let new_match_index = matches.len() - 1;
            searchable_item.update_matches(matches, Some(new_match_index), *token, window, cx);
            searchable_item.activate_match(new_match_index, matches, *token, window, cx);
        }
    }

    fn on_query_editor_event(
        &mut self,
        _editor: &Entity<Editor>,
        event: &editor::EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            editor::EditorEvent::Focused => self.query_editor_focused = true,
            editor::EditorEvent::Blurred => self.query_editor_focused = false,
            editor::EditorEvent::Edited { .. } => {
                self.smartcase(window, cx);
                self.clear_matches(window, cx);
                let search = self.update_matches(false, true, window, cx);

                cx.spawn_in(window, async move |this, cx| {
                    if search.await.is_ok() {
                        this.update_in(cx, |this, window, cx| {
                            this.activate_current_match(window, cx);
                            #[cfg(target_os = "macos")]
                            this.update_find_pasteboard(cx);
                        })?;
                    }
                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            }
            _ => {}
        }
    }

    fn on_replacement_editor_event(
        &mut self,
        _: Entity<Editor>,
        event: &editor::EditorEvent,
        _: &mut Context<Self>,
    ) {
        match event {
            editor::EditorEvent::Focused => self.replacement_editor_focused = true,
            editor::EditorEvent::Blurred => self.replacement_editor_focused = false,
            _ => {}
        }
    }

    fn on_active_searchable_item_event(
        &mut self,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchEvent::MatchesInvalidated => {
                drop(self.update_matches(false, false, window, cx));
            }
            SearchEvent::ActiveMatchChanged => self.update_match_index(window, cx),
        }
    }

    fn toggle_case_sensitive(
        &mut self,
        _: &ToggleCaseSensitive,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_search_option(SearchOptions::CASE_SENSITIVE, window, cx)
    }

    fn toggle_whole_word(
        &mut self,
        _: &ToggleWholeWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx)
    }

    fn toggle_selection(
        &mut self,
        _: &ToggleSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_search_within_selection(
            if let Some(_) = self.selection_search_enabled {
                None
            } else {
                Some(FilteredSearchRange::Default)
            },
            window,
            cx,
        );
    }

    fn toggle_regex(&mut self, _: &ToggleRegex, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_search_option(SearchOptions::REGEX, window, cx)
    }

    fn clear_active_searchable_item_matches(&mut self, window: &mut Window, cx: &mut App) {
        if let Some(active_searchable_item) = self.active_searchable_item.as_ref() {
            self.active_match_index = None;
            self.searchable_items_with_matches
                .remove(&active_searchable_item.downgrade());
            active_searchable_item.clear_matches(window, cx);
        }
    }

    pub fn has_active_match(&self) -> bool {
        self.active_match_index.is_some()
    }

    fn clear_matches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut active_item_matches = None;
        for (searchable_item, matches) in self.searchable_items_with_matches.drain() {
            if let Some(searchable_item) =
                WeakSearchableItemHandle::upgrade(searchable_item.as_ref(), cx)
            {
                if Some(&searchable_item) == self.active_searchable_item.as_ref() {
                    active_item_matches = Some((searchable_item.downgrade(), matches));
                } else {
                    searchable_item.clear_matches(window, cx);
                }
            }
        }

        self.searchable_items_with_matches
            .extend(active_item_matches);
    }

    fn update_matches(
        &mut self,
        reuse_existing_query: bool,
        add_to_history: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<()> {
        let (done_tx, done_rx) = oneshot::channel();
        let query = self.query(cx);
        self.pending_search.take();
        #[cfg(target_os = "macos")]
        self.pending_external_query.take();

        if let Some(active_searchable_item) = self.active_searchable_item.as_ref() {
            self.query_error = None;
            if query.is_empty() {
                self.clear_active_searchable_item_matches(window, cx);
                let _ = done_tx.send(());
                cx.notify();
            } else {
                let query: Arc<_> = if let Some(search) =
                    self.active_search.take().filter(|_| reuse_existing_query)
                {
                    search
                } else {
                    // Value doesn't matter, we only construct empty matchers with it

                    if self.search_options.contains(SearchOptions::REGEX) {
                        match SearchQuery::regex(
                            query,
                            self.search_options.contains(SearchOptions::WHOLE_WORD),
                            self.search_options.contains(SearchOptions::CASE_SENSITIVE),
                            false,
                            self.search_options
                                .contains(SearchOptions::ONE_MATCH_PER_LINE),
                            PathMatcher::default(),
                            PathMatcher::default(),
                            false,
                            None,
                        ) {
                            Ok(query) => query.with_replacement(self.replacement(cx)),
                            Err(e) => {
                                self.query_error = Some(e.to_string());
                                self.clear_active_searchable_item_matches(window, cx);
                                cx.notify();
                                return done_rx;
                            }
                        }
                    } else {
                        match SearchQuery::text(
                            query,
                            self.search_options.contains(SearchOptions::WHOLE_WORD),
                            self.search_options.contains(SearchOptions::CASE_SENSITIVE),
                            false,
                            PathMatcher::default(),
                            PathMatcher::default(),
                            false,
                            None,
                        ) {
                            Ok(query) => query.with_replacement(self.replacement(cx)),
                            Err(e) => {
                                self.query_error = Some(e.to_string());
                                self.clear_active_searchable_item_matches(window, cx);
                                cx.notify();
                                return done_rx;
                            }
                        }
                    }
                    .into()
                };

                self.active_search = Some(query.clone());
                let query_text = query.as_str().to_string();

                let matches_with_token =
                    active_searchable_item.find_matches_with_token(query, window, cx);

                let active_searchable_item = active_searchable_item.downgrade();
                self.pending_search = Some(cx.spawn_in(window, async move |this, cx| {
                    let (matches, token) = matches_with_token.await;

                    this.update_in(cx, |this, window, cx| {
                        if let Some(active_searchable_item) =
                            WeakSearchableItemHandle::upgrade(active_searchable_item.as_ref(), cx)
                        {
                            this.searchable_items_with_matches
                                .insert(active_searchable_item.downgrade(), (matches, token));

                            this.update_match_index(window, cx);

                            if add_to_history {
                                this.search_history
                                    .add(&mut this.search_history_cursor, query_text);
                            }
                            if !this.dismissed {
                                let (matches, token) = this
                                    .searchable_items_with_matches
                                    .get(&active_searchable_item.downgrade())
                                    .unwrap();
                                if matches.is_empty() {
                                    active_searchable_item.clear_matches(window, cx);
                                } else {
                                    active_searchable_item.update_matches(
                                        matches,
                                        this.active_match_index,
                                        *token,
                                        window,
                                        cx,
                                    );
                                }
                            }
                            let _ = done_tx.send(());
                            cx.notify();
                        }
                    })
                    .log_err();
                }));
            }
        }
        done_rx
    }

    fn reverse_direction_if_backwards(&self, direction: Direction) -> Direction {
        if self.search_options.contains(SearchOptions::BACKWARDS) {
            direction.opposite()
        } else {
            direction
        }
    }

    pub fn update_match_index(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let direction = self.reverse_direction_if_backwards(Direction::Next);
        let new_index = self
            .active_searchable_item
            .as_ref()
            .and_then(|searchable_item| {
                let (matches, token) = self
                    .searchable_items_with_matches
                    .get(&searchable_item.downgrade())?;
                searchable_item.active_match_index(direction, matches, *token, window, cx)
            });
        if new_index != self.active_match_index {
            self.active_match_index = new_index;
            if !self.dismissed {
                if let Some(searchable_item) = self.active_searchable_item.as_ref() {
                    if let Some((matches, token)) = self
                        .searchable_items_with_matches
                        .get(&searchable_item.downgrade())
                    {
                        if !matches.is_empty() {
                            searchable_item.update_matches(matches, new_index, *token, window, cx);
                        }
                    }
                }
            }
            cx.notify();
        }
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_field(Direction::Next, window, cx);
    }

    fn backtab(&mut self, _: &Backtab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_field(Direction::Prev, window, cx);
    }
    fn cycle_field(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let mut handles = vec![self.query_editor.focus_handle(cx)];
        if self.replace_enabled {
            handles.push(self.replacement_editor.focus_handle(cx));
        }
        if let Some(item) = self.active_searchable_item.as_ref() {
            handles.push(item.item_focus_handle(cx));
        }
        let current_index = match handles.iter().position(|focus| focus.is_focused(window)) {
            Some(index) => index,
            None => return,
        };

        let new_index = match direction {
            Direction::Next => (current_index + 1) % handles.len(),
            Direction::Prev if current_index == 0 => handles.len() - 1,
            Direction::Prev => (current_index - 1) % handles.len(),
        };
        let next_focus_handle = &handles[new_index];
        self.focus(next_focus_handle, window, cx);
        cx.stop_propagation();
    }

    fn next_history_query(
        &mut self,
        _: &NextHistoryQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !should_navigate_history(&self.query_editor, HistoryNavigationDirection::Next, cx) {
            cx.propagate();
            return;
        }

        if let Some(new_query) = self
            .search_history
            .next(&mut self.search_history_cursor)
            .map(str::to_string)
        {
            drop(self.search(&new_query, Some(self.search_options), false, window, cx));
        } else if let Some(draft) = self.search_history_cursor.take_draft() {
            drop(self.search(&draft, Some(self.search_options), false, window, cx));
        }
    }

    fn previous_history_query(
        &mut self,
        _: &PreviousHistoryQuery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !should_navigate_history(&self.query_editor, HistoryNavigationDirection::Previous, cx) {
            cx.propagate();
            return;
        }

        if self.query(cx).is_empty()
            && let Some(new_query) = self
                .search_history
                .current(&self.search_history_cursor)
                .map(str::to_string)
        {
            drop(self.search(&new_query, Some(self.search_options), false, window, cx));
            return;
        }

        let current_query = self.query(cx);
        if let Some(new_query) = self
            .search_history
            .previous(&mut self.search_history_cursor, &current_query)
            .map(str::to_string)
        {
            drop(self.search(&new_query, Some(self.search_options), false, window, cx));
        }
    }

    fn focus(&self, handle: &gpui::FocusHandle, window: &mut Window, cx: &mut App) {
        window.invalidate_character_coordinates();
        window.focus(handle, cx);
    }

    fn toggle_replace(&mut self, _: &ToggleReplace, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_searchable_item.is_some() {
            self.replace_enabled = !self.replace_enabled;
            let handle = if self.replace_enabled {
                self.replacement_editor.focus_handle(cx)
            } else {
                self.query_editor.focus_handle(cx)
            };
            self.focus(&handle, window, cx);
            cx.notify();
        }
    }

    fn replace_next(&mut self, _: &ReplaceNext, window: &mut Window, cx: &mut Context<Self>) {
        let mut should_propagate = true;
        if !self.dismissed
            && self.active_search.is_some()
            && let Some(searchable_item) = self.active_searchable_item.as_ref()
            && let Some(query) = self.active_search.as_ref()
            && let Some((matches, token)) = self
                .searchable_items_with_matches
                .get(&searchable_item.downgrade())
        {
            if let Some(active_index) = self.active_match_index {
                let query = query
                    .as_ref()
                    .clone()
                    .with_replacement(self.replacement(cx));
                searchable_item.replace(matches.at(active_index), &query, *token, window, cx);
                self.select_next_match(&SelectNextMatch, window, cx);
            }
            should_propagate = false;
        }
        if !should_propagate {
            cx.stop_propagation();
        }
    }

    pub fn replace_all(&mut self, _: &ReplaceAll, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dismissed
            && self.active_search.is_some()
            && let Some(searchable_item) = self.active_searchable_item.as_ref()
            && let Some(query) = self.active_search.as_ref()
            && let Some((matches, token)) = self
                .searchable_items_with_matches
                .get(&searchable_item.downgrade())
        {
            let query = query
                .as_ref()
                .clone()
                .with_replacement(self.replacement(cx));
            searchable_item.replace_all(&mut matches.iter(), &query, *token, window, cx);
        }
    }

    pub fn match_exists(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.update_match_index(window, cx);
        self.active_match_index.is_some()
    }

    pub fn should_use_smartcase_search(&mut self, cx: &mut Context<Self>) -> bool {
        EditorSettings::get_global(cx).use_smartcase_search
    }

    pub fn is_contains_uppercase(&mut self, str: &String) -> bool {
        str.chars().any(|c| c.is_uppercase())
    }

    fn smartcase(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.should_use_smartcase_search(cx) {
            let query = self.query(cx);
            if !query.is_empty() {
                let is_case = self.is_contains_uppercase(&query);
                if self.has_search_option(SearchOptions::CASE_SENSITIVE) != is_case {
                    self.toggle_search_option(SearchOptions::CASE_SENSITIVE, window, cx);
                }
            }
        }
    }

    fn adjust_query_regex_language(&self, cx: &mut App) {
        let enable = self.search_options.contains(SearchOptions::REGEX);
        let query_buffer = self
            .query_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .expect("query editor should be backed by a singleton buffer");

        if enable {
            if let Some(regex_language) = self.regex_language.clone() {
                query_buffer.update(cx, |query_buffer, cx| {
                    query_buffer.set_language(Some(regex_language), cx);
                })
            }
        } else {
            query_buffer.update(cx, |query_buffer, cx| {
                query_buffer.set_language(None, cx);
            })
        }
    }

    /// Updates the searchable item's case sensitivity option to match the
    /// search bar's current case sensitivity setting. This ensures that
    /// editor's `select_next`/ `select_previous` operations respect the buffer
    /// search bar's search options.
    ///
    /// Clears the case sensitivity when the search bar is dismissed so that
    /// only the editor's settings are respected.
    fn sync_select_next_case_sensitivity(&self, cx: &mut Context<Self>) {
        let case_sensitive = match self.dismissed {
            true => None,
            false => Some(self.search_options.contains(SearchOptions::CASE_SENSITIVE)),
        };

        if let Some(active_searchable_item) = self.active_searchable_item.as_ref() {
            active_searchable_item.set_search_is_case_sensitive(case_sensitive, cx);
        }
    }
}

#[cfg(test)]
mod tests;

use std::{
    cell::RefCell,
    cmp::{self},
    ops::{Not as _, Range},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

mod action_arguments_editor;
mod action_completion_provider;
#[path = "keymap_editor/modal_support.rs"]
mod modal_support;
mod ui_components;

use action_arguments_editor::ActionArgumentsEditor;
use anyhow::{Context as _, anyhow};
use collections::{HashMap, HashSet};
use editor::{CompletionProvider, Editor, EditorEvent, EditorMode, SizingBehavior};
use fs::Fs;
use fuzzy::{StringMatch, StringMatchCandidate};
use gpui::{
    Action, AppContext as _, AsyncApp, ClickEvent, Context, DismissEvent, Entity, EventEmitter,
    FocusHandle, Focusable, Global, IsZero,
    KeyBindingContextPredicate::{And, Descendant, Equal, Identifier, Not, NotEqual, Or},
    KeyContext, KeybindingKeystroke, MouseButton, PlatformKeyboardMapper, Point, ScrollStrategy,
    ScrollWheelEvent, Stateful, StyledText, Subscription, Task, TextStyleRefinement, WeakEntity,
    actions, anchored, deferred, div,
};
use language::{Language, LanguageConfig, ToOffset as _};
use modal_support::*;

use notifications::status_toast::StatusToast;
use project::{CompletionDisplayOptions, Project};
use settings::{
    BaseKeymap, KeybindSource, KeymapFile, Settings as _, SettingsAssets, infer_json_indent_size,
};
use ui::{
    ActiveTheme as _, App, Banner, BorrowAppContext, ColumnWidthConfig, ContextMenu,
    IconButtonShape, IconPosition, Indicator, Modal, ModalFooter, ModalHeader, ParentElement as _,
    PopoverMenu, RedistributableColumnsState, Render, Section, SharedString, Styled as _, Table,
    TableInteractionState, TableResizeBehavior, Tooltip, Window, prelude::*,
};
use ui_input::InputField;
use util::ResultExt;
use workspace::{
    Item, ModalView, SerializableItem, Workspace, notifications::NotifyTaskExt as _,
    register_serializable_item, with_active_or_new_workspace,
};

use mav_actions::{ChangeKeybinding, OpenKeymap};
pub use ui_components::*;

use crate::{
    action_completion_provider::ActionCompletionProvider,
    persistence::KeybindingEditorDb,
    ui_components::keystroke_input::{
        ClearKeystrokes, KeystrokeInput, StartRecording, StopRecording,
    },
};

const NO_ACTION_ARGUMENTS_TEXT: SharedString = SharedString::new_static("<no arguments>");
const COLS: usize = 6;

actions!(
    keymap_editor,
    [
        /// Edits the selected key binding.
        EditBinding,
        /// Creates a new key binding for the selected action.
        CreateBinding,
        /// Creates a new key binding from scratch, prompting for the action.
        OpenCreateKeybindingModal,
        /// Deletes the selected key binding.
        DeleteBinding,
        /// Copies the action name to clipboard.
        CopyAction,
        /// Copies the context predicate to clipboard.
        CopyContext,
        /// Toggles Conflict Filtering
        ToggleConflictFilter,
        /// Toggles whether NoAction bindings are shown
        ToggleNoActionBindings,
        /// Toggle Keystroke search
        ToggleKeystrokeSearch,
        /// Toggles exact matching for keystroke search
        ToggleExactKeystrokeMatching,
        /// Shows matching keystrokes for the currently selected binding
        ShowMatchingKeybinds
    ]
);

pub fn init(cx: &mut App) {
    let keymap_event_channel = KeymapEventChannel::new();
    cx.set_global(keymap_event_channel);

    fn open_keymap_editor(
        filter: Option<String>,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let existing = workspace
            .active_pane()
            .read(cx)
            .items()
            .find_map(|item| item.downcast::<KeymapEditor>());

        let keymap_editor = if let Some(existing) = existing {
            workspace.activate_item(&existing, true, true, window, cx);
            existing
        } else {
            let keymap_editor = cx.new(|cx| KeymapEditor::new(workspace.weak_handle(), window, cx));
            workspace.add_item_to_active_pane(
                Box::new(keymap_editor.clone()),
                None,
                true,
                window,
                cx,
            );
            keymap_editor
        };

        if let Some(filter) = filter {
            keymap_editor.update(cx, |editor, cx| {
                editor.filter_editor.update(cx, |editor, cx| {
                    editor.clear(window, cx);
                    editor.insert(&filter, window, cx);
                });
                if !editor.has_binding_for(&filter) {
                    open_binding_modal_after_loading(cx)
                }
            })
        }
    }

    cx.on_action(|_: &OpenKeymap, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_keymap_editor(None, workspace, window, cx);
        });
    });

    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, action: &ChangeKeybinding, window, cx| {
            open_keymap_editor(Some(action.action.clone()), workspace, window, cx);
        });
    })
    .detach();

    register_serializable_item::<KeymapEditor>(cx);
}

fn open_binding_modal_after_loading(cx: &mut Context<KeymapEditor>) {
    let started_at = Instant::now();
    let observer = Rc::new(RefCell::new(None));
    let handle = {
        let observer = Rc::clone(&observer);
        cx.observe(&cx.entity(), move |editor, _, cx| {
            let subscription = observer.borrow_mut().take();

            if started_at.elapsed().as_secs() > 10 {
                return;
            }
            if !editor.matches.is_empty() {
                editor.selected_index = Some(0);
                cx.dispatch_action(&CreateBinding);
                return;
            }

            *observer.borrow_mut() = subscription;
        })
    };
    *observer.borrow_mut() = Some(handle);
}

pub struct KeymapEventChannel {}

impl Global for KeymapEventChannel {}

impl KeymapEventChannel {
    fn new() -> Self {
        Self {}
    }

    pub fn trigger_keymap_changed(cx: &mut App) {
        let Some(_event_channel) = cx.try_global::<Self>() else {
            // don't panic if no global defined. This usually happens in tests
            return;
        };
        cx.update_global(|_event_channel: &mut Self, _| {
            /* triggers observers in KeymapEditors */
        });
    }
}

#[derive(Default, PartialEq, Copy, Clone)]
enum SearchMode {
    #[default]
    Normal,
    KeyStroke {
        exact_match: bool,
    },
}

impl SearchMode {
    fn invert(&self) -> Self {
        match self {
            SearchMode::Normal => SearchMode::KeyStroke { exact_match: true },
            SearchMode::KeyStroke { .. } => SearchMode::Normal,
        }
    }

    fn exact_match(&self) -> bool {
        match self {
            SearchMode::Normal => false,
            SearchMode::KeyStroke { exact_match } => *exact_match,
        }
    }
}

#[derive(Default, PartialEq, Copy, Clone)]
enum FilterState {
    #[default]
    All,
    Conflicts,
}

impl FilterState {
    fn invert(&self) -> Self {
        match self {
            FilterState::All => FilterState::Conflicts,
            FilterState::Conflicts => FilterState::All,
        }
    }
}

#[derive(Default, PartialEq, Eq, Copy, Clone)]
struct SourceFilters {
    user: bool,
    mav_defaults: bool,
    vim_defaults: bool,
}

impl SourceFilters {
    fn allows(&self, source: Option<KeybindSource>) -> bool {
        match source {
            Some(KeybindSource::User) => self.user,
            Some(KeybindSource::Vim) => self.vim_defaults,
            Some(KeybindSource::Base | KeybindSource::Default | KeybindSource::Unknown) | None => {
                self.mav_defaults
            }
        }
    }
}

#[path = "keymap_editor/binding_model.rs"]
mod binding_model;
#[path = "keymap_editor/conflicts.rs"]
mod conflicts;
mod editor_actions_filters;
mod editor_core;
mod editor_selection_menu;
mod item_render;
mod modal_focus_state;
mod modal_render;
mod modal_save_focus;
mod modal_setup;
mod render_helpers;
mod serialization;
use binding_model::*;
use conflicts::{ActionMapping, ConflictOrigin, ConflictState, KeybindConflict};
use render_helpers::*;

struct KeymapEditor {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    _keymap_subscription: Subscription,
    keybindings: Vec<ProcessedBinding>,
    keybinding_conflict_state: ConflictState,
    filter_state: FilterState,
    source_filters: SourceFilters,
    show_no_action_bindings: bool,
    search_mode: SearchMode,
    search_query_debounce: Option<Task<()>>,
    // corresponds 1 to 1 with keybindings
    string_match_candidates: Arc<Vec<StringMatchCandidate>>,
    matches: Vec<StringMatch>,
    table_interaction_state: Entity<TableInteractionState>,
    filter_editor: Entity<Editor>,
    keystroke_editor: Entity<KeystrokeInput>,
    selected_index: Option<usize>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    previous_edit: Option<PreviousEdit>,
    humanized_action_names: HumanizedActionNameCache,
    current_widths: Entity<RedistributableColumnsState>,
    show_hover_menus: bool,
    actions_with_schemas: HashSet<&'static str>,
    /// In order for the JSON LSP to run in the actions arguments editor, we
    /// require a backing file In order to avoid issues (primarily log spam)
    /// with drop order between the buffer, file, worktree, etc, we create a
    /// temporary directory for these backing files in the keymap editor struct
    /// instead of here. This has the added benefit of only having to create a
    /// worktree and directory once, although the perf improvement is negligible.
    action_args_temp_dir_worktree: Option<Entity<project::Worktree>>,
    action_args_temp_dir: Option<tempfile::TempDir>,
}

enum PreviousEdit {
    /// When deleting, we want to maintain the same scroll position
    ScrollBarOffset(Point<Pixels>),
    /// When editing or creating, because the new keybinding could be in a different position in the sort order
    /// we store metadata about the new binding (either the modified version or newly created one)
    /// and upon reload, we search for this binding in the list of keybindings, and if we find the one that matches
    /// this metadata, we set the selected index to it and scroll to it,
    /// and if we don't find it, we scroll to 0 and don't set a selected index
    Keybinding {
        action_mapping: ActionMapping,
        action_name: &'static str,
        /// The scrollbar position to fallback to if we don't find the keybinding during a refresh
        /// this can happen if there's a filter applied to the search and the keybinding modification
        /// filters the binding from the search results
        fallback: Point<Pixels>,
    },
}

impl EventEmitter<()> for KeymapEditor {}

impl Focusable for KeymapEditor {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        if self.selected_index.is_some() {
            self.focus_handle.clone()
        } else {
            self.filter_editor.focus_handle(cx)
        }
    }
}
/// Helper function to check if two keystroke sequences match exactly
fn keystrokes_match_exactly(
    keystrokes1: &[KeybindingKeystroke],
    keystrokes2: &[KeybindingKeystroke],
) -> bool {
    keystrokes1.len() == keystrokes2.len()
        && keystrokes1.iter().zip(keystrokes2).all(|(k1, k2)| {
            k1.inner().key == k2.inner().key && k1.inner().modifiers == k2.inner().modifiers
        })
}

fn disabled_binding_matches_context(
    disabled_binding: &gpui::KeyBinding,
    binding: &gpui::KeyBinding,
) -> bool {
    match (
        disabled_binding.predicate().as_deref(),
        binding.predicate().as_deref(),
    ) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(disabled_predicate), Some(predicate)) => disabled_predicate.is_superset(predicate),
    }
}

fn binding_is_unbound_by_unbind(
    binding: &gpui::KeyBinding,
    binding_index: usize,
    all_bindings: &[&gpui::KeyBinding],
) -> bool {
    all_bindings[binding_index + 1..]
        .iter()
        .rev()
        .any(|disabled_binding| {
            gpui::is_unbind(disabled_binding.action())
                && keystrokes_match_exactly(disabled_binding.keystrokes(), binding.keystrokes())
                && disabled_binding
                    .action()
                    .as_any()
                    .downcast_ref::<gpui::Unbind>()
                    .is_some_and(|unbind| unbind.0.as_ref() == binding.action().name())
                && disabled_binding_matches_context(disabled_binding, binding)
        })
}

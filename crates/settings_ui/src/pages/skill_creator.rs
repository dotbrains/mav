use agent_skills::{
    AGENTS_DIR_NAME, MAX_SKILL_DESCRIPTION_LEN, MAX_SKILL_FILE_SIZE, SKILL_FILE_NAME,
    SKILLS_DIR_NAME, SkillMetadata, SkillsUpdatedHook, global_skills_dir, parse_skill_file_content,
    slugify_skill_name, validate_description, validate_name,
};
use anyhow::{Context as _, Result, anyhow};
use editor::{CurrentLineHighlight, Editor, EditorElement, EditorEvent, EditorStyle};
use fs::Fs;
use futures::AsyncReadExt;
use gpui::{
    App, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle, Subscription, Task, TextStyle,
    WeakEntity, WindowHandle, actions,
};
use http_client::{AsyncBody, HttpClient, HttpRequestExt, Request, StatusCode, Url};
use language::{Buffer, language_settings::SoftWrap};
use settings::{ActionSequence, Settings};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use theme_settings::ThemeSettings;
use ui::{Banner, Divider, SwitchField, WithScrollbar, prelude::*};
use ui_input::{ErasedEditorEvent, InputField};
use util::ResultExt;
use workspace::MultiWorkspace;

use crate::{SettingsUiFile, SettingsWindow, all_projects};

mod github_import;
mod persistence;
mod render;
mod save;
mod state;
mod url_import;

#[cfg(test)]
mod tests;

use github_import::is_supported_skill_url;

actions!(
    skill_creator,
    [SaveSkill, Cancel, FocusNextField, FocusPreviousField,]
);

const URL_FIELD_TAB_INDEX: isize = 1;
const NAME_FIELD_TAB_INDEX: isize = 2;
const DESCRIPTION_FIELD_TAB_INDEX: isize = 3;
const DISABLE_MODEL_INVOCATION_TAB_INDEX: isize = 4;
const BODY_FIELD_TAB_INDEX: isize = 5;
const SAVE_BUTTON_TAB_INDEX: isize = 6;
const URL_IMPORT_DEBOUNCE: Duration = Duration::from_millis(300);
const URL_IMPORT_ERROR_BODY_MAX_LEN: usize = 2048;

#[derive(Clone, Debug, Default)]
pub enum SkillCreatorOpenMode {
    #[default]
    Form,
    Url {
        initial_url: Option<String>,
    },
    Install {
        content: String,
    },
}

pub(crate) enum SkillCreatorEvent {
    Dismissed,
    Saved,
}

#[derive(Clone, Debug)]
enum UrlImportStatus {
    Idle,
    Fetching,
    Error(SharedString),
}

#[derive(Debug)]
struct ImportedSkill {
    name: String,
    description: String,
    body: String,
    disable_model_invocation: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum ScopeChoice {
    Global,
    Project {
        root_name: SharedString,
        abs_path: Arc<std::path::Path>,
    },
}

impl ScopeChoice {
    /// Absolute path of the `.agents/skills` directory this scope writes to.
    fn skills_dir(&self) -> PathBuf {
        match self {
            ScopeChoice::Global => global_skills_dir(),
            ScopeChoice::Project { abs_path, .. } => {
                abs_path.join(AGENTS_DIR_NAME).join(SKILLS_DIR_NAME)
            }
        }
    }
}

fn scope_for_settings_file(
    current_file: &SettingsUiFile,
    original_window: Option<&WindowHandle<MultiWorkspace>>,
    cx: &App,
) -> ScopeChoice {
    if let SettingsUiFile::Project((worktree_id, _)) = current_file {
        for project in all_projects(original_window, cx) {
            if let Some(worktree) = project.read(cx).worktree_for_id(*worktree_id, cx) {
                let worktree = worktree.read(cx);
                return ScopeChoice::Project {
                    root_name: SharedString::from(worktree.root_name_str().to_string()),
                    abs_path: worktree.abs_path(),
                };
            }
        }
    }
    ScopeChoice::Global
}

pub(crate) fn skill_url_from_clipboard(cx: &App) -> Option<String> {
    cx.read_from_clipboard()
        .and_then(|clipboard| clipboard.text())
        .map(|text| text.trim().to_string())
        .filter(|text| is_supported_skill_url(text))
}

/// Renders the skill creator sub-page pushed by
/// [`SettingsWindow::open_skill_creator_sub_page`].
pub(crate) fn render_skill_creator_page(
    settings_window: &SettingsWindow,
    _scroll_handle: &ScrollHandle,
    _window: &mut Window,
    _cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let Some(page) = settings_window.skill_creator_page() else {
        return gpui::Empty.into_any_element();
    };
    page.into_any_element()
}

pub struct SkillCreatorPage {
    focus_handle: FocusHandle,
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    url_editor: Entity<InputField>,
    name_editor: Entity<InputField>,
    description_editor: Entity<InputField>,
    body_editor: Entity<Editor>,
    description_length: usize,
    settings_window: WeakEntity<SettingsWindow>,
    disable_model_invocation: bool,
    name_error: Option<&'static str>,
    description_error: Option<&'static str>,
    body_error: Option<&'static str>,
    save_error: Option<SharedString>,
    url_import_status: UrlImportStatus,
    saving: bool,
    save_task: Option<Task<()>>,
    url_import_debounce_task: Option<Task<()>>,
    url_import_task: Option<Task<()>>,
    scroll_handle: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SkillCreatorEvent> for SkillCreatorPage {}

impl SkillCreatorPage {
    pub(crate) fn new(
        settings_window: WeakEntity<SettingsWindow>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let app_state = workspace::AppState::global(cx);
        let fs = app_state.fs.clone();
        let language_registry = app_state.languages.clone();
        let http_client = cx.http_client();

        let focus_handle = cx.focus_handle();

        let url_editor = cx.new(|cx| {
            InputField::new(
                window,
                cx,
                "https://github.com/owner/repo/blob/main/path/to/SKILL.md",
            )
            .tab_index(URL_FIELD_TAB_INDEX)
            .tab_stop(true)
        });

        let name_editor = cx.new(|cx| {
            InputField::new(window, cx, "my-new-skill")
                .label("Name")
                .tab_index(NAME_FIELD_TAB_INDEX)
                .tab_stop(true)
        });
        // Focus the name field on open.
        window.focus(&name_editor.focus_handle(cx), cx);

        let description_editor = cx.new(|cx| {
            InputField::new(
                window,
                cx,
                "e.g., Fill the PR description following this template.",
            )
            .label("Description")
            .tab_index(DESCRIPTION_FIELD_TAB_INDEX)
            .tab_stop(true)
        });

        let body_editor = cx.new(|cx| {
            let buffer = cx.new(|cx| {
                let buffer = Buffer::local(String::new(), cx);
                buffer.set_language_registry(language_registry.clone());
                buffer
            });
            let mut editor = Editor::for_buffer(buffer, None, window, cx);
            editor.set_placeholder_text("Add skill content…", window, cx);
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
            editor.set_show_gutter(false, cx);
            editor.set_show_wrap_guides(false, cx);
            editor.set_show_indent_guides(false, cx);
            editor.set_use_modal_editing(true);
            editor.set_current_line_highlight(Some(CurrentLineHighlight::None));
            editor
        });

        cx.spawn_in(window, {
            let body_editor = body_editor.downgrade();
            let language_registry = language_registry.clone();
            async move |_this, cx| {
                let markdown = language_registry.language_for_name("Markdown").await.ok();
                if let Some(markdown) = markdown {
                    body_editor
                        .update(cx, |editor, cx| {
                            editor.buffer().update(cx, |multi_buffer, cx| {
                                if let Some(buffer) = multi_buffer.as_singleton() {
                                    buffer.update(cx, |buffer, cx| {
                                        buffer.set_language(Some(markdown), cx)
                                    });
                                }
                            });
                        })
                        .ok();
                }
            }
        })
        .detach();

        let url_input_editor = url_editor.read(cx).editor().clone();
        let name_input_editor = name_editor.read(cx).editor().clone();
        let description_input_editor = description_editor.read(cx).editor().clone();
        let weak = cx.weak_entity();
        let url_subscription = url_input_editor.subscribe(
            Box::new(move |event, window, cx| {
                weak.update(cx, |this, cx| {
                    this.handle_url_input_event(&event, window, cx);
                })
                .ok();
            }),
            window,
            cx,
        );
        let weak = cx.weak_entity();
        let name_subscription = name_input_editor.subscribe(
            Box::new(move |event, window, cx| {
                weak.update(cx, |this, cx| {
                    this.handle_name_input_event(&event, window, cx);
                })
                .ok();
            }),
            window,
            cx,
        );
        let weak = cx.weak_entity();
        let description_subscription = description_input_editor.subscribe(
            Box::new(move |event, window, cx| {
                weak.update(cx, |this, cx| {
                    this.handle_description_input_event(&event, window, cx);
                })
                .ok();
            }),
            window,
            cx,
        );

        let subscriptions = vec![
            url_subscription,
            name_subscription,
            description_subscription,
            cx.subscribe_in(&body_editor, window, Self::handle_body_editor_event),
        ];

        Self {
            focus_handle,
            fs,
            http_client,
            url_editor,
            name_editor,
            description_editor,
            body_editor,
            description_length: 0,
            settings_window,
            disable_model_invocation: false,
            name_error: None,
            description_error: None,
            body_error: None,
            save_error: None,
            url_import_status: UrlImportStatus::Idle,
            saving: false,
            save_task: None,
            url_import_debounce_task: None,
            url_import_task: None,
            scroll_handle: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }
}

impl Focusable for SkillCreatorPage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

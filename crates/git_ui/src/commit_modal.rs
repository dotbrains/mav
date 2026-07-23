use crate::branch_picker::{self, BranchList};
use crate::git_panel::{
    GitPanel, commit_message_editor, commit_title_exceeds_limit, git_commit_editor_style,
};
use crate::git_panel_settings::GitPanelSettings;
use git::repository::CommitOptions;
use git::{Amend, Commit, GenerateCommitMessage, Signoff};
use mav_actions::{DecreaseBufferFontSize, IncreaseBufferFontSize, ResetBufferFontSize};
use project::DisableAiSettings;
use settings::Settings;
use ui::{
    ContextMenu, KeybindingHint, PopoverMenu, PopoverMenuHandle, SplitButton, Tooltip, prelude::*,
};

use editor::{Editor, EditorElement};
use gpui::*;
use util::ResultExt;
use workspace::{
    ModalView, Workspace,
    dock::{Dock, PanelHandle},
};

// nate: It is a pain to get editors to size correctly and not overflow.
//
// this can get replaced with a simple flex layout with more time/a more thoughtful approach.
#[derive(Debug, Clone, Copy)]
pub struct ModalContainerProperties {
    pub modal_width: f32,
    pub editor_height: f32,
    pub footer_height: f32,
    pub container_padding: f32,
    pub modal_border_radius: f32,
}

impl ModalContainerProperties {
    pub fn new(window: &Window, preferred_char_width: usize) -> Self {
        let container_padding = 5.0;

        // Calculate width based on character width
        let mut modal_width = 460.0;
        let style = window.text_style();
        let font_id = window.text_system().resolve_font(&style.font());
        let font_size = style.font_size.to_pixels(window.rem_size());

        if let Ok(em_width) = window.text_system().em_width(font_id, font_size) {
            modal_width =
                f32::from(preferred_char_width as f32 * em_width + px(container_padding * 2.0));
        }

        Self {
            modal_width,
            editor_height: 300.0,
            footer_height: 24.0,
            container_padding,
            modal_border_radius: 12.0,
        }
    }

    pub fn editor_border_radius(&self) -> Pixels {
        px(self.modal_border_radius - self.container_padding / 2.0)
    }
}

pub struct CommitModal {
    git_panel: Entity<GitPanel>,
    commit_editor: Entity<Editor>,
    restore_dock: RestoreDock,
    properties: ModalContainerProperties,
    branch_list_handle: PopoverMenuHandle<BranchList>,
    commit_menu_handle: PopoverMenuHandle<ContextMenu>,
}

impl Focusable for CommitModal {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.commit_editor.focus_handle(cx)
    }
}

impl EventEmitter<DismissEvent> for CommitModal {}
impl ModalView for CommitModal {
    fn on_before_dismiss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> workspace::DismissDecision {
        self.git_panel.update(cx, |git_panel, cx| {
            git_panel.set_modal_open(false, cx);
        });
        self.restore_dock
            .dock
            .update(cx, |dock, cx| {
                if let Some(active_index) = self.restore_dock.active_index {
                    dock.activate_panel(active_index, window, cx)
                }
                dock.set_open(self.restore_dock.is_open, window, cx)
            })
            .log_err();
        workspace::DismissDecision::Dismiss(true)
    }
}

struct RestoreDock {
    dock: WeakEntity<Dock>,
    is_open: bool,
    active_index: Option<usize>,
}

pub enum ForceMode {
    Amend,
    Commit,
}

impl CommitModal {
    pub fn register(workspace: &mut Workspace) {
        workspace.register_action(|workspace, _: &Commit, window, cx| {
            CommitModal::toggle(workspace, Some(ForceMode::Commit), window, cx);
        });
        workspace.register_action(|workspace, _: &Amend, window, cx| {
            CommitModal::toggle(workspace, Some(ForceMode::Amend), window, cx);
        });
    }

    pub fn toggle(
        workspace: &mut Workspace,
        force_mode: Option<ForceMode>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let Some(git_panel) = workspace.panel::<GitPanel>(cx) else {
            return;
        };

        git_panel.update(cx, |git_panel, cx| {
            if let Some(force_mode) = force_mode {
                match force_mode {
                    ForceMode::Amend => {
                        if git_panel
                            .active_repository
                            .as_ref()
                            .and_then(|repo| repo.read(cx).head_commit.as_ref())
                            .is_some()
                            && !git_panel.amend_pending()
                        {
                            git_panel.set_amend_pending(true, cx);
                            git_panel.load_last_commit_message(cx);
                        }
                    }
                    ForceMode::Commit => {
                        if git_panel.amend_pending() {
                            git_panel.set_amend_pending(false, cx);
                        }
                    }
                }
            }
            git_panel.set_modal_open(true, cx);
            git_panel.load_local_committer(cx);
        });

        let dock = workspace.dock_at_position(git_panel.position(window, cx));
        let is_open = dock.read(cx).is_open();
        let active_index = dock.read(cx).active_panel_index();
        let dock = dock.downgrade();
        let restore_dock_position = RestoreDock {
            dock,
            is_open,
            active_index,
        };

        workspace.open_panel::<GitPanel>(window, cx);
        workspace.toggle_modal(window, cx, move |window, cx| {
            CommitModal::new(git_panel, restore_dock_position, window, cx)
        })
    }

    fn new(
        git_panel: Entity<GitPanel>,
        restore_dock: RestoreDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let panel = git_panel.read(cx);
        let suggested_commit_message = panel.suggest_commit_message(cx);

        let commit_editor = git_panel.update(cx, |git_panel, cx| {
            git_panel.set_modal_open(true, cx);
            let buffer = git_panel.commit_message_buffer(cx);
            let panel_editor = git_panel.commit_editor.clone();
            let project = git_panel.project.clone();

            cx.new(|cx| {
                let mut editor =
                    commit_message_editor(buffer, None, project.clone(), false, window, cx);
                editor.sync_selections(panel_editor, cx).detach();

                editor
            })
        });

        let commit_message = commit_editor.read(cx).text(cx);

        if let Some(suggested_commit_message) = suggested_commit_message
            && commit_message.is_empty()
        {
            commit_editor.update(cx, |editor, cx| {
                editor.set_placeholder_text(&suggested_commit_message, window, cx);
            });
        }

        let focus_handle = commit_editor.focus_handle(cx);

        cx.on_focus_out(&focus_handle, window, |this, _, window, cx| {
            if !this.branch_list_handle.is_focused(window, cx)
                && !this.commit_menu_handle.is_focused(window, cx)
            {
                cx.emit(DismissEvent);
            }
        })
        .detach();

        let properties = ModalContainerProperties::new(window, 50);

        Self {
            git_panel,
            commit_editor,
            restore_dock,
            properties,
            branch_list_handle: PopoverMenuHandle::default(),
            commit_menu_handle: PopoverMenuHandle::default(),
        }
    }

    fn dismiss(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.git_panel.read(cx).amend_pending() {
            self.git_panel
                .update(cx, |git_panel, cx| git_panel.set_amend_pending(false, cx));
        } else {
            cx.emit(DismissEvent);
        }
    }

    fn on_commit(&mut self, _: &git::Commit, window: &mut Window, cx: &mut Context<Self>) {
        let is_amend = self.git_panel.read(cx).amend_pending();
        let did_execute = self.git_panel.update(cx, |git_panel, cx| {
            git_panel.commit(&self.commit_editor.focus_handle(cx), window, cx)
        });
        if did_execute {
            if is_amend {
                telemetry::event!("Git Amended", source = "Git Modal");
            } else {
                telemetry::event!("Git Committed", source = "Git Modal");
            }
            cx.emit(DismissEvent);
        }
    }

    fn on_amend(&mut self, _: &git::Amend, window: &mut Window, cx: &mut Context<Self>) {
        if self.git_panel.update(cx, |git_panel, cx| {
            git_panel.amend(&self.commit_editor.focus_handle(cx), window, cx)
        }) {
            telemetry::event!("Git Amended", source = "Git Modal");
            cx.emit(DismissEvent);
        }
    }

    fn toggle_branch_selector(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.branch_list_handle.is_focused(window, cx) {
            self.focus_handle(cx).focus(window, cx)
        } else {
            self.branch_list_handle.toggle(window, cx);
        }
    }

    fn increase_font_size(
        &mut self,
        action: &IncreaseBufferFontSize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_panel.update(cx, |git_panel, cx| {
            git_panel.increase_font_size(action, window, cx);
        });
    }

    fn decrease_font_size(
        &mut self,
        action: &DecreaseBufferFontSize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_panel.update(cx, |git_panel, cx| {
            git_panel.decrease_font_size(action, window, cx);
        });
    }

    fn reset_font_size(
        &mut self,
        action: &ResetBufferFontSize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_panel.update(cx, |git_panel, cx| {
            git_panel.reset_font_size(action, window, cx);
        });
    }
}

mod render;

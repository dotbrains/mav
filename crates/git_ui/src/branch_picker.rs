use anyhow::Context as _;
use editor::Editor;
use fuzzy_nucleo::StringMatchCandidate;

use collections::HashSet;
use git::repository::{Branch, delete_branch_flag};
use gpui::http_client::Url;
use gpui::{
    Action, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, Modifiers, ModifiersChangedEvent, ParentElement, PromptLevel,
    Render, SharedString, Styled, Subscription, Task, TaskExt, WeakEntity, Window, actions, rems,
};
use picker::{Picker, PickerDelegate, PickerEditorPosition};
use project::git_store::{Repository, RepositoryEvent};
use project::project_settings::ProjectSettings;
use settings::Settings;
use std::sync::Arc;
use time::OffsetDateTime;
use ui::{
    Banner, Divider, HighlightedLabel, KeyBinding, ListItem, ListItemSpacing, Severity, Tooltip,
    prelude::*,
};
use ui_input::ErasedEditor;
use util::ResultExt;
use workspace::notifications::DetachAndPromptErr;
use workspace::{ModalView, Workspace};

use crate::{branch_picker, git_panel::show_error_toast};

actions!(
    branch_picker,
    [
        /// Deletes the selected git branch or remote.
        DeleteBranch,
        /// Force deletes the selected git branch or remote.
        ForceDeleteBranch,
        /// Filter the list of remotes
        FilterRemotes
    ]
);

pub fn checkout_branch(
    workspace: &mut Workspace,
    _: &mav_actions::git::CheckoutBranch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    open(workspace, &mav_actions::git::Branch, window, cx);
}

pub fn switch(
    workspace: &mut Workspace,
    _: &mav_actions::git::Switch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    open(workspace, &mav_actions::git::Branch, window, cx);
}

pub fn open(
    workspace: &mut Workspace,
    _: &mav_actions::git::Branch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let workspace_handle = workspace.weak_handle();
    let repository = workspace.project().read(cx).active_repository(cx);

    workspace.toggle_modal(window, cx, |window, cx| {
        BranchList::new(
            workspace_handle,
            repository,
            BranchListStyle::Modal,
            rems(34.),
            window,
            cx,
        )
    })
}

pub fn popover(
    workspace: WeakEntity<Workspace>,
    modal_style: bool,
    repository: Option<Entity<Repository>>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<BranchList> {
    let (style, width) = if modal_style {
        (BranchListStyle::Modal, rems(34.))
    } else {
        (BranchListStyle::Popover, rems(20.))
    };

    cx.new(|cx| {
        let list = BranchList::new(workspace, repository, style, width, window, cx);
        list.focus_handle(cx).focus(window, cx);
        list
    })
}

pub fn select_popover(
    workspace: WeakEntity<Workspace>,
    repository: Option<Entity<Repository>>,
    selected_branch: Option<SharedString>,
    on_select: SelectBranchCallback,
    window: &mut Window,
    cx: &mut App,
) -> Entity<BranchList> {
    cx.new(|cx| {
        let list = BranchList::new_select(
            workspace,
            repository,
            BranchListStyle::Popover,
            rems(20.),
            selected_branch,
            on_select,
            window,
            cx,
        );
        list.focus_handle(cx).focus(window, cx);
        list
    })
}

pub fn select_modal(
    workspace: WeakEntity<Workspace>,
    repository: Option<Entity<Repository>>,
    selected_branch: Option<SharedString>,
    on_select: SelectBranchCallback,
    window: &mut Window,
    cx: &mut Context<BranchList>,
) -> BranchList {
    let list = BranchList::new_select(
        workspace,
        repository,
        BranchListStyle::Modal,
        rems(34.),
        selected_branch,
        on_select,
        window,
        cx,
    );
    list.focus_handle(cx).focus(window, cx);
    list
}

pub type SelectBranchCallback = Arc<dyn Fn(Branch, &mut Window, &mut App)>;

pub fn create_embedded(
    workspace: WeakEntity<Workspace>,
    repository: Option<Entity<Repository>>,
    width: Rems,
    show_footer: bool,
    window: &mut Window,
    cx: &mut Context<BranchList>,
) -> BranchList {
    BranchList::new_embedded(workspace, repository, width, show_footer, window, cx)
}

#[path = "branch_picker/delegate_actions.rs"]
mod delegate_actions;
#[path = "branch_picker/entries.rs"]
mod entries;
#[path = "branch_picker/list.rs"]
mod list;
#[path = "branch_picker/picker_delegate.rs"]
mod picker_delegate;
#[path = "branch_picker/render_footer.rs"]
mod render_footer;
#[path = "branch_picker/render_match.rs"]
mod render_match;
#[cfg(test)]
#[path = "branch_picker/tests.rs"]
mod tests;

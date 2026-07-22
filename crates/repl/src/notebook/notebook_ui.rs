#![allow(unused, dead_code)]
use std::future::Future;
use std::{path::PathBuf, sync::Arc};

use anyhow::{Context as _, Result};
use client::proto::ViewId;
use collections::HashMap;
use editor::DisplayPoint;
use feature_flags::{FeatureFlagAppExt as _, NotebookFeatureFlag};
use futures::FutureExt;
use futures::future::Shared;
use gpui::{
    AnyElement, App, Entity, EventEmitter, FocusHandle, Focusable, KeyContext, ListScrollEvent,
    ListState, Point, Task, TaskExt, actions, list, prelude::*,
};
use jupyter_protocol::JupyterKernelspec;
use language::{Language, LanguageRegistry};
use log;
use project::{Project, ProjectEntryId, ProjectPath};
use settings::Settings as _;
use ui::{CommonAnimationExt, Tooltip, prelude::*};
use workspace::item::{ItemEvent, SaveOptions, TabContentParams};
use workspace::searchable::SearchableItemHandle;
use workspace::{Item, ItemHandle, Pane, ProjectItem, ToolbarItemLocation};

use super::{Cell, CellEvent, CellPosition, MarkdownCellEvent, RenderableCell};

use nbformat::v4::CellId;
use nbformat::v4::Metadata as NotebookMetadata;
use serde_json;
use uuid::Uuid;

use crate::components::{KernelPickerDelegate, KernelSelector};
use crate::kernels::{
    Kernel, KernelSession, KernelSpecification, KernelStatus, LocalKernelSpecification,
    NativeRunningKernel, RemoteRunningKernel, SshRunningKernel, WslRunningKernel,
};
use crate::notebook::MovementDirection;
use crate::repl_store::ReplStore;

use mav_actions::editor::{MoveDown, MoveUp};
use mav_actions::notebook::{
    AddCodeBlock, AddMarkdownBlock, ClearOutputs, EnterCommandMode, EnterEditMode, InterruptKernel,
    MoveCellDown, MoveCellUp, NotebookMoveDown, NotebookMoveUp, OpenNotebook, RestartKernel, Run,
    RunAll, RunAndAdvance,
};
use picker::Picker;
use runtimelib::{ExecuteRequest, JupyterMessage, JupyterMessageContent};
use ui::PopoverMenuHandle;

mod cells;
mod kernel;
mod kernel_session;
mod notebook_item;
mod render;
mod render_helpers;
mod workspace_item;

#[cfg(test)]
mod tests;

pub use notebook_item::NotebookItem;

/// Whether the notebook is in command mode (navigating cells) or edit mode (editing a cell).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotebookMode {
    Command,
    Edit,
}

#[derive(PartialEq, Eq)]
enum SelectionMode {
    SelectOnly,
    SelectAndMove,
}

pub(crate) const MAX_TEXT_BLOCK_WIDTH: f32 = 9999.0;
pub(crate) const SMALL_SPACING_SIZE: f32 = 8.0;
pub(crate) const MEDIUM_SPACING_SIZE: f32 = 12.0;
pub(crate) const LARGE_SPACING_SIZE: f32 = 16.0;
pub(crate) const GUTTER_WIDTH: f32 = 19.0;
pub(crate) const CODE_BLOCK_INSET: f32 = MEDIUM_SPACING_SIZE;
pub(crate) const CONTROL_SIZE: f32 = 20.0;

pub fn init(cx: &mut App) {
    if cx.has_flag::<NotebookFeatureFlag>() || std::env::var("LOCAL_NOTEBOOK_DEV").is_ok() {
        workspace::register_project_item::<NotebookEditor>(cx);
    }

    cx.observe_flag::<NotebookFeatureFlag, _>({
        move |flag, cx| {
            if *flag {
                workspace::register_project_item::<NotebookEditor>(cx);
            } else {
                // todo: there is no way to unregister a project item, so if the feature flag
                // gets turned off they need to restart Mav.
            }
        }
    })
    .detach();
}

pub struct NotebookEditor {
    languages: Arc<LanguageRegistry>,
    project: Entity<Project>,
    worktree_id: project::WorktreeId,
    focus_handle: FocusHandle,
    notebook_item: Entity<NotebookItem>,
    notebook_language: Shared<Task<Option<Arc<Language>>>>,
    remote_id: Option<ViewId>,
    cell_list: ListState,
    notebook_mode: NotebookMode,
    selected_cell_index: usize,
    cell_order: Vec<CellId>,
    original_cell_order: Vec<CellId>,
    cell_map: HashMap<CellId, Cell>,
    kernel: Kernel,
    kernel_specification: Option<KernelSpecification>,
    execution_requests: HashMap<String, CellId>,
    kernel_picker_handle: PopoverMenuHandle<Picker<KernelPickerDelegate>>,
}

impl NotebookEditor {
    pub fn new(
        project: Entity<Project>,
        notebook_item: Entity<NotebookItem>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let languages = project.read(cx).languages().clone();
        let language_name = notebook_item.read(cx).language_name();
        let worktree_id = notebook_item.read(cx).project_path.worktree_id;

        let notebook_language = notebook_item.read(cx).notebook_language();
        let notebook_language = cx
            .spawn_in(window, async move |_, _| notebook_language.await)
            .shared();

        let mut cell_order = vec![]; // Vec<CellId>
        let mut cell_map = HashMap::default(); // HashMap<CellId, Cell>

        let cell_count = notebook_item.read(cx).notebook.cells.len();
        for index in 0..cell_count {
            let cell = notebook_item.read(cx).notebook.cells[index].clone();
            let cell_id = cell.id();
            cell_order.push(cell_id.clone());
            let cell_entity = Cell::load(&cell, &languages, notebook_language.clone(), window, cx);

            match &cell_entity {
                Cell::Code(code_cell) => {
                    let cell_id_for_focus = cell_id.clone();
                    cx.subscribe_in(code_cell, window, move |this, _cell, event, window, cx| {
                        match event {
                            CellEvent::Run(cell_id) => {
                                this.execute_cell(cell_id.clone(), window, cx)
                            }
                            CellEvent::FocusedIn(_) => {
                                this.select_cell_by_id(&cell_id_for_focus, cx)
                            }
                        }
                    })
                    .detach();

                    let cell_id_for_editor = cell_id.clone();
                    let editor = code_cell.read(cx).editor().clone();
                    cx.subscribe(&editor, move |this, _editor, event, cx| {
                        if let editor::EditorEvent::Focused = event {
                            this.select_cell_by_id(&cell_id_for_editor, cx);
                        }
                    })
                    .detach();
                }
                Cell::Markdown(markdown_cell) => {
                    cx.subscribe(
                        markdown_cell,
                        move |_this, cell, event: &MarkdownCellEvent, cx| {
                            match event {
                                MarkdownCellEvent::FinishedEditing => {
                                    cell.update(cx, |cell, cx| {
                                        cell.reparse_markdown(cx);
                                    });
                                }
                                MarkdownCellEvent::Run(_cell_id) => {
                                    // run is handled separately by move_to_next_cell
                                    // Just reparse here
                                    cell.update(cx, |cell, cx| {
                                        cell.reparse_markdown(cx);
                                    });
                                }
                            }
                        },
                    )
                    .detach();

                    let cell_id_for_editor = cell_id.clone();
                    let editor = markdown_cell.read(cx).editor().clone();
                    cx.subscribe(&editor, move |this, _editor, event, cx| {
                        if let editor::EditorEvent::Focused = event {
                            this.select_cell_by_id(&cell_id_for_editor, cx);
                        }
                    })
                    .detach();
                }
                Cell::Raw(_) => {}
            }

            cell_map.insert(cell_id.clone(), cell_entity);
        }

        let notebook_handle = cx.entity().downgrade();
        let cell_count = cell_order.len();

        let this = cx.entity();
        let cell_list = ListState::new(cell_count, gpui::ListAlignment::Top, px(1000.));

        let mut editor = Self {
            project,
            languages: languages.clone(),
            worktree_id,
            focus_handle,
            notebook_item: notebook_item.clone(),
            notebook_language,
            remote_id: None,
            cell_list,
            notebook_mode: NotebookMode::Command,
            selected_cell_index: 0,
            cell_order: cell_order.clone(),
            original_cell_order: cell_order.clone(),
            cell_map: cell_map.clone(),
            kernel: Kernel::Shutdown,
            kernel_specification: None,
            execution_requests: HashMap::default(),
            kernel_picker_handle: PopoverMenuHandle::default(),
        };
        editor.launch_kernel(window, cx);
        editor.refresh_language(cx);
        editor.refresh_kernelspecs(cx);

        cx.subscribe(&notebook_item, |this, _item, _event, cx| {
            this.refresh_language(cx);
        })
        .detach();

        editor
    }

    fn refresh_kernelspecs(&mut self, cx: &mut Context<Self>) {
        let store = ReplStore::global(cx);
        let project = self.project.clone();
        let worktree_id = self.worktree_id;

        let refresh_task = store.update(cx, |store, cx| {
            store.refresh_python_kernelspecs(worktree_id, &project, cx)
        });

        cx.background_spawn(refresh_task).detach_and_log_err(cx);
    }

    fn refresh_language(&mut self, cx: &mut Context<Self>) {
        let notebook_language = self.notebook_item.read(cx).notebook_language();
        let task = cx.spawn(async move |this, cx| {
            let language = notebook_language.await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    for cell in this.cell_map.values() {
                        if let Cell::Code(code_cell) = cell {
                            code_cell.update(cx, |cell, cx| {
                                cell.set_language(language.clone(), cx);
                            });
                        }
                    }
                });
            }
            language
        });
        self.notebook_language = task.shared();
    }

    fn has_structural_changes(&self) -> bool {
        self.cell_order != self.original_cell_order
    }

    fn has_content_changes(&self, cx: &App) -> bool {
        self.cell_map.values().any(|cell| cell.is_dirty(cx))
    }

    pub fn to_notebook(&self, cx: &App) -> nbformat::v4::Notebook {
        let cells: Vec<nbformat::v4::Cell> = self
            .cell_order
            .iter()
            .filter_map(|cell_id| {
                self.cell_map
                    .get(cell_id)
                    .map(|cell| cell.to_nbformat_cell(cx))
            })
            .collect();

        let metadata = self.notebook_item.read(cx).notebook.metadata.clone();

        nbformat::v4::Notebook {
            metadata,
            nbformat: 4,
            nbformat_minor: 5,
            cells,
        }
    }

    pub fn mark_as_saved(&mut self, cx: &mut Context<Self>) {
        self.original_cell_order = self.cell_order.clone();

        for cell in self.cell_map.values() {
            match cell {
                Cell::Code(code_cell) => {
                    code_cell.update(cx, |code_cell, cx| {
                        let editor = code_cell.editor();
                        editor.update(cx, |editor, cx| {
                            editor.buffer().update(cx, |buffer, cx| {
                                if let Some(buf) = buffer.as_singleton() {
                                    buf.update(cx, |b, cx| {
                                        let version = b.version();
                                        b.did_save(version, None, cx);
                                    });
                                }
                            });
                        });
                    });
                }
                Cell::Markdown(markdown_cell) => {
                    markdown_cell.update(cx, |markdown_cell, cx| {
                        let editor = markdown_cell.editor();
                        editor.update(cx, |editor, cx| {
                            editor.buffer().update(cx, |buffer, cx| {
                                if let Some(buf) = buffer.as_singleton() {
                                    buf.update(cx, |b, cx| {
                                        let version = b.version();
                                        b.did_save(version, None, cx);
                                    });
                                }
                            });
                        });
                    });
                }
                Cell::Raw(_) => {}
            }
        }
        cx.notify();
    }
}

impl Focusable for NotebookEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for NotebookEditor {}

// pub struct NotebookControls {
//     pane_focused: bool,
//     active_item: Option<Box<dyn ItemHandle>>,
//     // subscription: Option<Subscription>,
// }

// impl NotebookControls {
//     pub fn new() -> Self {
//         Self {
//             pane_focused: false,
//             active_item: Default::default(),
//             // subscription: Default::default(),
//         }
//     }
// }

// impl EventEmitter<ToolbarItemEvent> for NotebookControls {}

// impl Render for NotebookControls {
//     fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//         div().child("notebook controls")
//     }
// }

// impl ToolbarItemView for NotebookControls {
//     fn set_active_pane_item(
//         &mut self,
//         active_pane_item: Option<&dyn workspace::ItemHandle>,
//         window: &mut Window, cx: &mut Context<Self>,
//     ) -> workspace::ToolbarItemLocation {
//         cx.notify();
//         self.active_item = None;

//         let Some(item) = active_pane_item else {
//             return ToolbarItemLocation::Hidden;
//         };

//         ToolbarItemLocation::PrimaryLeft
//     }

//     fn pane_focus_update(&mut self, pane_focused: bool, _window: &mut Window, _cx: &mut Context<Self>) {
//         self.pane_focused = pane_focused;
//     }
// }

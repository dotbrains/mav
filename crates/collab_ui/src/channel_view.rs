use anyhow::Result;
use call::ActiveCall;
use channel::{Channel, ChannelBuffer, ChannelBufferEvent, ChannelStore};
use client::ChannelId;
use editor::{
    DisplayPoint, Editor, EditorEvent, SelectionEffects, display_map::ToDisplayPoint,
    scroll::Autoscroll,
};
use gpui::{
    App, ClipboardItem, Context, Entity, EventEmitter, Focusable, Render, Subscription, Task,
    VisualContext as _, WeakEntity, Window, actions,
};
use project::Project;
use std::sync::Arc;
use ui::prelude::*;
use util::ResultExt;
use workspace::notifications::NotificationId;
use workspace::{Pane, SaveIntent, Toast, ViewId, Workspace, item::ItemEvent};

use collaboration_hub::ChannelBufferCollaborationHub;

mod collaboration_hub;
mod followable;
mod item;

actions!(
    collab,
    [
        /// Copies a link to the current position in the channel buffer.
        CopyLink
    ]
);

pub fn init(cx: &mut App) {
    workspace::FollowableViewRegistry::register::<ChannelView>(cx)
}

pub struct ChannelView {
    pub editor: Entity<Editor>,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    channel_store: Entity<ChannelStore>,
    channel_buffer: Entity<ChannelBuffer>,
    remote_id: Option<ViewId>,
    _editor_event_subscription: Subscription,
    _reparse_subscription: Option<Subscription>,
}

impl ChannelView {
    pub fn open(
        channel_id: ChannelId,
        link_position: Option<String>,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let pane = workspace.read(cx).active_pane().clone();
        let channel_view = Self::open_in_pane(
            channel_id,
            link_position,
            pane.clone(),
            workspace,
            window,
            cx,
        );
        window.spawn(cx, async move |cx| {
            let channel_view = channel_view.await?;
            pane.update_in(cx, |pane, window, cx| {
                telemetry::event!(
                    "Channel Notes Opened",
                    channel_id,
                    room_id = ActiveCall::global(cx)
                        .read(cx)
                        .room()
                        .map(|r| r.read(cx).id())
                );
                pane.add_item(Box::new(channel_view.clone()), true, true, None, window, cx);
            })?;
            anyhow::Ok(channel_view)
        })
    }

    pub fn open_in_pane(
        channel_id: ChannelId,
        link_position: Option<String>,
        pane: Entity<Pane>,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let channel_view = Self::load(channel_id, workspace, window, cx);
        window.spawn(cx, async move |cx| {
            let channel_view = channel_view.await?;

            pane.update_in(cx, |pane, window, cx| {
                let buffer_id = channel_view.read(cx).channel_buffer.read(cx).remote_id(cx);

                let existing_view = pane
                    .items_of_type::<Self>()
                    .find(|view| view.read(cx).channel_buffer.read(cx).remote_id(cx) == buffer_id);

                // If this channel buffer is already open in this pane, just return it.
                if let Some(existing_view) = existing_view.clone()
                    && existing_view.read(cx).channel_buffer == channel_view.read(cx).channel_buffer
                {
                    if let Some(link_position) = link_position {
                        existing_view.update(cx, |channel_view, cx| {
                            channel_view.focus_position_from_link(link_position, true, window, cx)
                        });
                    }
                    return existing_view;
                }

                // If the pane contained a disconnected view for this channel buffer,
                // replace that.
                if let Some(existing_item) = existing_view
                    && let Some(ix) = pane.index_for_item(&existing_item)
                {
                    pane.close_item_by_id(existing_item.entity_id(), SaveIntent::Skip, window, cx)
                        .detach();
                    pane.add_item(
                        Box::new(channel_view.clone()),
                        true,
                        true,
                        Some(ix),
                        window,
                        cx,
                    );
                }

                if let Some(link_position) = link_position {
                    channel_view.update(cx, |channel_view, cx| {
                        channel_view.focus_position_from_link(link_position, true, window, cx)
                    });
                }

                channel_view
            })
        })
    }

    pub fn load(
        channel_id: ChannelId,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<Entity<Self>>> {
        let weak_workspace = workspace.downgrade();
        let workspace = workspace.read(cx);
        let project = workspace.project().to_owned();
        let channel_store = ChannelStore::global(cx);
        let language_registry = workspace.app_state().languages.clone();
        let markdown = language_registry.language_for_name("Markdown");
        let channel_buffer =
            channel_store.update(cx, |store, cx| store.open_channel_buffer(channel_id, cx));

        window.spawn(cx, async move |cx| {
            let channel_buffer = channel_buffer.await?;
            let markdown = markdown.await.log_err();

            channel_buffer.update(cx, |channel_buffer, cx| {
                channel_buffer.buffer().update(cx, |buffer, cx| {
                    buffer.set_language_registry(language_registry);
                    let Some(markdown) = markdown else {
                        return;
                    };
                    buffer.set_language(Some(markdown), cx);
                })
            });

            cx.new_window_entity(|window, cx| {
                let mut this = Self::new(
                    project,
                    weak_workspace,
                    channel_store,
                    channel_buffer,
                    window,
                    cx,
                );
                this.acknowledge_buffer_version(cx);
                this
            })
        })
    }

    pub fn new(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        channel_store: Entity<ChannelStore>,
        channel_buffer: Entity<ChannelBuffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let buffer = channel_buffer.read(cx).buffer();
        let this = cx.entity().downgrade();
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_buffer(buffer, None, window, cx);
            editor.set_collaboration_hub(Box::new(ChannelBufferCollaborationHub(
                channel_buffer.clone(),
            )));
            editor.set_custom_context_menu(move |_, position, window, cx| {
                let this = this.clone();
                Some(ui::ContextMenu::build(window, cx, move |menu, _, _| {
                    menu.entry("Copy Link to Section", None, move |window, cx| {
                        this.update(cx, |this, cx| {
                            this.copy_link_for_position(position, window, cx)
                        })
                        .ok();
                    })
                }))
            });
            editor.set_show_bookmarks(false, cx);
            editor.set_show_breakpoints(false, cx);
            editor.set_show_runnables(false, cx);
            editor
        });
        let _editor_event_subscription =
            cx.subscribe(&editor, |_, _, e: &EditorEvent, cx| cx.emit(e.clone()));

        cx.subscribe_in(&channel_buffer, window, Self::handle_channel_buffer_event)
            .detach();

        Self {
            editor,
            workspace,
            project,
            channel_store,
            channel_buffer,
            remote_id: None,
            _editor_event_subscription,
            _reparse_subscription: None,
        }
    }

    fn focus_position_from_link(
        &mut self,
        position: String,
        first_attempt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = Channel::slug(&position).to_lowercase();
        let snapshot = self
            .editor
            .update(cx, |editor, cx| editor.snapshot(window, cx));

        if let Some(outline) = snapshot.buffer_snapshot().outline(None)
            && let Some(item) = outline
                .items
                .iter()
                .find(|item| &Channel::slug(&item.text).to_lowercase() == &position)
        {
            self.editor.update(cx, |editor, cx| {
                editor.change_selections(
                    SelectionEffects::scroll(Autoscroll::focused()),
                    window,
                    cx,
                    |s| s.replace_cursors_with(|map| vec![item.range.start.to_display_point(map)]),
                )
            });
            return;
        }

        if !first_attempt {
            return;
        }
        self._reparse_subscription = Some(cx.subscribe_in(
            &self.editor,
            window,
            move |this, _, e: &EditorEvent, window, cx| {
                match e {
                    EditorEvent::Reparsed(_) => {
                        this.focus_position_from_link(position.clone(), false, window, cx);
                        this._reparse_subscription.take();
                    }
                    EditorEvent::Edited { .. } | EditorEvent::SelectionsChanged { local: true } => {
                        this._reparse_subscription.take();
                    }
                    _ => {}
                };
            },
        ));
    }

    fn copy_link(&mut self, _: &CopyLink, window: &mut Window, cx: &mut Context<Self>) {
        let position = self.editor.update(cx, |editor, cx| {
            editor
                .selections
                .newest_display(&editor.display_snapshot(cx))
                .start
        });
        self.copy_link_for_position(position, window, cx)
    }

    fn copy_link_for_position(
        &self,
        position: DisplayPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self
            .editor
            .update(cx, |editor, cx| editor.snapshot(window, cx));

        let mut closest_heading = None;

        if let Some(outline) = snapshot.buffer_snapshot().outline(None) {
            for item in outline.items {
                if item.range.start.to_display_point(&snapshot) > position {
                    break;
                }
                closest_heading = Some(item);
            }
        }

        let Some(channel) = self.channel(cx) else {
            return;
        };

        let link = channel.notes_link(closest_heading.map(|heading| heading.text.to_string()), cx);
        cx.write_to_clipboard(ClipboardItem::new_string(link));
        self.workspace
            .update(cx, |workspace, cx| {
                struct CopyLinkForPositionToast;

                workspace.show_toast(
                    Toast::new(
                        NotificationId::unique::<CopyLinkForPositionToast>(),
                        "Link copied to clipboard",
                    ),
                    cx,
                );
            })
            .ok();
    }

    pub fn channel(&self, cx: &App) -> Option<Arc<Channel>> {
        self.channel_buffer.read(cx).channel(cx)
    }

    fn handle_channel_buffer_event(
        &mut self,
        _: &Entity<ChannelBuffer>,
        event: &ChannelBufferEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChannelBufferEvent::Disconnected => self.editor.update(cx, |editor, cx| {
                editor.set_read_only(true);
                cx.notify();
            }),
            ChannelBufferEvent::Connected => self.editor.update(cx, |editor, cx| {
                editor.set_read_only(false);
                cx.notify();
            }),
            ChannelBufferEvent::ChannelChanged => {
                self.editor.update(cx, |_, cx| {
                    cx.emit(editor::EditorEvent::TitleChanged);
                    cx.notify()
                });
            }
            ChannelBufferEvent::BufferEdited => {
                if self.editor.read(cx).is_focused(window) {
                    self.acknowledge_buffer_version(cx);
                } else {
                    self.channel_store.update(cx, |store, cx| {
                        let channel_buffer = self.channel_buffer.read(cx);
                        store.update_latest_notes_version(
                            channel_buffer.channel_id,
                            channel_buffer.epoch(),
                            &channel_buffer.buffer().read(cx).version(),
                            cx,
                        )
                    });
                }
            }
            ChannelBufferEvent::CollaboratorsChanged => {}
        }
    }

    fn acknowledge_buffer_version(&mut self, cx: &mut Context<ChannelView>) {
        self.channel_store.update(cx, |store, cx| {
            let channel_buffer = self.channel_buffer.read(cx);
            store.acknowledge_notes_version(
                channel_buffer.channel_id,
                channel_buffer.epoch(),
                &channel_buffer.buffer().read(cx).version(),
                cx,
            )
        });
        self.channel_buffer.update(cx, |buffer, cx| {
            buffer.acknowledge_buffer_version(cx);
        });
    }

    fn get_channel(&self, cx: &App) -> (SharedString, Option<SharedString>) {
        if let Some(channel) = self.channel(cx) {
            let status = match (
                self.channel_buffer.read(cx).buffer().read(cx).read_only(),
                self.channel_buffer.read(cx).is_connected(),
            ) {
                (false, true) => None,
                (true, true) => Some("read-only"),
                (_, false) => Some("disconnected"),
            };

            (channel.name.clone(), status.map(Into::into))
        } else {
            ("<unknown>".into(), Some("disconnected".into()))
        }
    }
}

impl EventEmitter<EditorEvent> for ChannelView {}

impl Render for ChannelView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(cx.listener(Self::copy_link))
            .child(self.editor.clone())
    }
}

impl Focusable for ChannelView {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }
}

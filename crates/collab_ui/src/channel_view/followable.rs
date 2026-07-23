use super::ChannelView;
use client::{ChannelId, proto};
use editor::{Editor, EditorEvent};
use gpui::{App, Context, Entity, VisualContext as _, Window};
use project::Project;
use std::sync::Arc;
use workspace::{
    CollaboratorId,
    item::{Dedup, FollowableItem},
};

impl FollowableItem for ChannelView {
    fn remote_id(&self) -> Option<workspace::ViewId> {
        self.remote_id
    }

    fn to_state_proto(&self, window: &mut Window, cx: &mut App) -> Option<proto::view::Variant> {
        let (is_connected, channel_id) = {
            let channel_buffer = self.channel_buffer.read(cx);
            (channel_buffer.is_connected(), channel_buffer.channel_id.0)
        };
        if !is_connected {
            return None;
        }

        let editor_proto = self
            .editor
            .update(cx, |editor, cx| editor.to_state_proto(window, cx));
        Some(proto::view::Variant::ChannelView(
            proto::view::ChannelView {
                channel_id,
                editor: if let Some(proto::view::Variant::Editor(proto)) = editor_proto {
                    Some(proto)
                } else {
                    None
                },
            },
        ))
    }

    fn from_state_proto(
        workspace: Entity<workspace::Workspace>,
        remote_id: workspace::ViewId,
        state: &mut Option<proto::view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<gpui::Task<anyhow::Result<Entity<Self>>>> {
        let Some(proto::view::Variant::ChannelView(_)) = state else {
            return None;
        };
        let Some(proto::view::Variant::ChannelView(state)) = state.take() else {
            unreachable!()
        };

        let open = ChannelView::load(ChannelId(state.channel_id), workspace, window, cx);

        Some(window.spawn(cx, async move |cx| {
            let this = open.await?;

            let task = this.update_in(cx, |this, window, cx| {
                this.remote_id = Some(remote_id);

                if let Some(state) = state.editor {
                    Some(this.editor.update(cx, |editor, cx| {
                        editor.apply_update_proto(
                            &this.project,
                            proto::update_view::Variant::Editor(proto::update_view::Editor {
                                selections: state.selections,
                                pending_selection: state.pending_selection,
                                scroll_top_anchor: state.scroll_top_anchor,
                                scroll_x: state.scroll_x,
                                scroll_y: state.scroll_y,
                                ..Default::default()
                            }),
                            window,
                            cx,
                        )
                    }))
                } else {
                    None
                }
            })?;

            if let Some(task) = task {
                task.await?;
            }

            Ok(this)
        }))
    }

    fn add_event_to_update_proto(
        &self,
        event: &EditorEvent,
        update: &mut Option<proto::update_view::Variant>,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.editor.update(cx, |editor, cx| {
            editor.add_event_to_update_proto(event, update, window, cx)
        })
    }

    fn apply_update_proto(
        &mut self,
        project: &Entity<Project>,
        message: proto::update_view::Variant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        self.editor.update(cx, |editor, cx| {
            editor.apply_update_proto(project, message, window, cx)
        })
    }

    fn set_leader_id(
        &mut self,
        leader_id: Option<CollaboratorId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.set_leader_id(leader_id, window, cx))
    }

    fn is_project_item(&self, _window: &Window, _cx: &App) -> bool {
        false
    }

    fn to_follow_event(event: &Self::Event) -> Option<workspace::item::FollowEvent> {
        Editor::to_follow_event(event)
    }

    fn dedup(&self, existing: &Self, _: &Window, cx: &App) -> Option<Dedup> {
        let existing = existing.channel_buffer.read(cx);
        if self.channel_buffer.read(cx).channel_id == existing.channel_id {
            if existing.is_connected() {
                Some(Dedup::KeepExisting)
            } else {
                Some(Dedup::ReplaceExisting)
            }
        } else {
            None
        }
    }
}

use std::{
    any::{Any, TypeId},
    sync::Arc,
};

use super::ChannelView;
use editor::{Editor, EditorEvent};
use gpui::{App, Context, Entity, Pixels, Point, SharedString, Task, Window};
use rpc::proto::ChannelVisibility;
use ui::prelude::*;
use workspace::{
    ItemNavHistory, WorkspaceId,
    item::{Item, ItemEvent, TabContentParams},
    searchable::SearchableItemHandle,
};

impl Item for ChannelView {
    type Event = EditorEvent;

    fn act_as_type<'a>(
        &'a self,
        type_id: TypeId,
        self_handle: &'a Entity<Self>,
        _: &'a App,
    ) -> Option<gpui::AnyEntity> {
        if type_id == TypeId::of::<Self>() {
            Some(self_handle.clone().into())
        } else if type_id == TypeId::of::<Editor>() {
            Some(self.editor.clone().into())
        } else {
            None
        }
    }

    fn tab_icon(&self, _: &Window, cx: &App) -> Option<Icon> {
        let channel = self.channel(cx)?;
        let icon = match channel.visibility {
            ChannelVisibility::Public => IconName::Public,
            ChannelVisibility::Members => IconName::Hash,
        };

        Some(Icon::new(icon))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        let (name, status) = self.get_channel(cx);
        if let Some(status) = status {
            format!("{name} - {status}").into()
        } else {
            name
        }
    }

    fn tab_content(&self, params: TabContentParams, _: &Window, cx: &App) -> gpui::AnyElement {
        let (name, status) = self.get_channel(cx);
        h_flex()
            .gap_2()
            .child(
                Label::new(name)
                    .color(params.text_color())
                    .when(params.preview, |this| this.italic()),
            )
            .when_some(status, |element, status| {
                element.child(
                    Label::new(status)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .into_any_element()
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        None
    }

    fn can_split(&self) -> bool {
        true
    }

    fn clone_on_split(
        &self,
        _: Option<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Option<Entity<Self>>> {
        Task::ready(Some(cx.new(|cx| {
            Self::new(
                self.project.clone(),
                self.workspace.clone(),
                self.channel_store.clone(),
                self.channel_buffer.clone(),
                window,
                cx,
            )
        })))
    }

    fn navigate(
        &mut self,
        data: Arc<dyn Any + Send>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.editor
            .update(cx, |editor, cx| editor.navigate(data, window, cx))
    }

    fn deactivated(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |item, cx| item.deactivated(window, cx))
    }

    fn set_nav_history(
        &mut self,
        history: ItemNavHistory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            Item::set_nav_history(editor, history, window, cx)
        })
    }

    fn as_searchable(&self, _: &Entity<Self>, _: &App) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(self.editor.clone()))
    }

    fn show_toolbar(&self) -> bool {
        true
    }

    fn pixel_position_of_cursor(&self, cx: &App) -> Option<Point<Pixels>> {
        self.editor.read(cx).pixel_position_of_cursor(cx)
    }

    fn to_item_events(event: &EditorEvent, f: &mut dyn FnMut(ItemEvent)) {
        Editor::to_item_events(event, f)
    }
}

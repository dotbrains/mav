use client::telemetry::Telemetry;
use gpui::{Context, Empty, Entity, EventEmitter, IntoElement, Render, Window, prelude::*};
use ui::{IconButton, IconName, IconSize, Tooltip, prelude::*};
use workspace::{ItemHandle, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView};

use super::TelemetryLogView;

pub struct TelemetryLogToolbarItemView {
    telemetry_log: Option<Entity<TelemetryLogView>>,
    search_editor: Entity<editor::Editor>,
}

impl TelemetryLogToolbarItemView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_editor = cx.new(|cx| {
            let mut editor = editor::Editor::single_line(window, cx);
            editor.set_placeholder_text("Filter events...", window, cx);
            editor
        });

        cx.subscribe(
            &search_editor,
            |this, editor, event: &editor::EditorEvent, cx| {
                if let editor::EditorEvent::BufferEdited { .. } = event {
                    let query = editor.read(cx).text(cx);
                    if let Some(telemetry_log) = &this.telemetry_log {
                        telemetry_log.update(cx, |log, cx| {
                            log.set_search_query(query, cx);
                        });
                    }
                }
            },
        )
        .detach();

        Self {
            telemetry_log: None,
            search_editor,
        }
    }
}

impl Render for TelemetryLogToolbarItemView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(telemetry_log) = self.telemetry_log.as_ref() else {
            return Empty.into_any_element();
        };

        let telemetry_log_clone = telemetry_log.clone();
        let has_events = !telemetry_log.read(cx).events.is_empty();

        h_flex()
            .gap_2()
            .child(div().w(px(200.)).child(self.search_editor.clone()))
            .child(
                IconButton::new("clear_events", IconName::Trash)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Clear Events"))
                    .disabled(!has_events)
                    .on_click(cx.listener(move |_this, _, _window, cx| {
                        telemetry_log_clone.update(cx, |log, cx| {
                            log.clear_events(cx);
                        });
                    })),
            )
            .child(
                IconButton::new("open_log_file", IconName::File)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Open Raw Log File"))
                    .on_click(|_, _window, cx| {
                        let path = Telemetry::log_file_path();
                        cx.open_url(&format!("file://{}", path.display()));
                    }),
            )
            .into_any()
    }
}

impl EventEmitter<ToolbarItemEvent> for TelemetryLogToolbarItemView {}

impl ToolbarItemView for TelemetryLogToolbarItemView {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        if let Some(item) = active_pane_item
            && let Some(telemetry_log) = item.downcast::<TelemetryLogView>()
        {
            self.telemetry_log = Some(telemetry_log);
            cx.notify();
            return ToolbarItemLocation::PrimaryRight;
        }
        if self.telemetry_log.take().is_some() {
            cx.notify();
        }
        ToolbarItemLocation::Hidden
    }
}

use std::collections::VecDeque;
use time::OffsetDateTime;

use collections::HashSet;
use fs::Fs;
use futures::StreamExt;
use gpui::{
    App, Empty, Entity, EventEmitter, FocusHandle, Focusable, ListAlignment, ListState,
    StyleRefinement, Task, TextStyleRefinement, Window, list, prelude::*,
};
use markdown::{
    CodeBlockRenderer, CopyButtonVisibility, MarkdownElement, MarkdownStyle, WrapButtonVisibility,
};
use project::Project;
use settings::Settings;
use telemetry_events::EventWrapper;
use theme_settings::ThemeSettings;
use ui::{Icon, IconName, IconSize, Label, TextSize, WithScrollbar, prelude::*};
use workspace::{Item, Toast, Workspace, notifications::NotificationId};

#[path = "telemetry_log/entry.rs"]
mod entry;
#[path = "telemetry_log/toolbar.rs"]
mod toolbar;

use entry::{TelemetryLogEntry, event_wrapper_to_entry, expanded_params_md};
pub use toolbar::TelemetryLogToolbarItemView;

const MAX_EVENTS: usize = 10_000;

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _cx: &mut Context<Workspace>| {
            workspace.register_action(
                |workspace, _: &mav_actions::OpenTelemetryLog, window, cx| {
                    let telemetry_log =
                        cx.new(|cx| TelemetryLogView::new(workspace.project().clone(), window, cx));

                    cx.subscribe(&telemetry_log, |workspace, _, event, cx| {
                        let TelemetryLogEvent::ShowToast(toast) = event;
                        workspace.show_toast(toast.clone(), cx);
                    })
                    .detach();

                    workspace.add_item_to_active_pane(
                        Box::new(telemetry_log),
                        None,
                        true,
                        window,
                        cx,
                    );
                },
            );
        },
    )
    .detach();
}

pub struct TelemetryLogView {
    project: Entity<Project>,
    focus_handle: FocusHandle,
    events: VecDeque<TelemetryLogEntry>,
    list_state: ListState,
    expanded: HashSet<usize>,
    search_query: String,
    filtered_indices: Vec<usize>,
    _subscription: Task<()>,
}

impl TelemetryLogView {
    pub fn new(project: Entity<Project>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let telemetry = client::Client::global(cx).telemetry().clone();
        let fs = <dyn Fs>::global(cx);

        let list_state = ListState::new(0, ListAlignment::Bottom, px(2048.));

        let subscription = cx.spawn(async move |this, cx| {
            let subscription = telemetry.subscribe_with_history(fs).await;

            this.update(cx, |this, cx| {
                let historical_events = match subscription.historical_events {
                    Ok(historical) => {
                        if historical.parse_error_count > 0 {
                            this.show_parse_error_toast(historical.parse_error_count, cx);
                        }
                        historical.events
                    }
                    Err(err) => {
                        this.show_read_error_toast(&err, cx);
                        Vec::new()
                    }
                };

                this.push_events(
                    historical_events
                        .into_iter()
                        .chain(subscription.queued_events),
                    cx,
                );
            })
            .ok();

            let mut live_events = subscription.live_events;
            while let Some(event_wrapper) = live_events.next().await {
                let result = this.update(cx, |this, cx| {
                    this.push_event(event_wrapper, cx);
                });
                if result.is_err() {
                    break;
                }
            }
        });

        Self {
            project,
            focus_handle: cx.focus_handle(),
            events: VecDeque::with_capacity(MAX_EVENTS),
            list_state,
            expanded: HashSet::default(),
            search_query: String::new(),
            filtered_indices: Vec::new(),
            _subscription: subscription,
        }
    }

    fn push_event(&mut self, event_wrapper: EventWrapper, cx: &mut Context<Self>) {
        self.push_events(std::iter::once(event_wrapper), cx);
    }

    fn push_events(
        &mut self,
        event_wrappers: impl Iterator<Item = EventWrapper>,
        cx: &mut Context<Self>,
    ) {
        let language_registry = self.project.read(cx).languages().clone();

        for event_wrapper in event_wrappers {
            let entry = event_wrapper_to_entry(&event_wrapper, &language_registry, cx);
            self.events.push_back(entry);
        }

        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }

        self.expanded.retain(|&idx| idx < self.events.len());

        self.recompute_filtered_indices();
        cx.notify();
    }

    fn entry_matches_filter(&self, entry: &TelemetryLogEntry) -> bool {
        if self.search_query.is_empty() {
            return true;
        }

        let query_lower = self.search_query.to_lowercase();

        if entry.event_type.to_lowercase().contains(&query_lower) {
            return true;
        }

        for (key, value) in &entry.event_properties {
            if key.to_lowercase().contains(&query_lower) {
                return true;
            }
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if value_str.to_lowercase().contains(&query_lower) {
                return true;
            }
        }

        false
    }

    fn recompute_filtered_indices(&mut self) {
        self.filtered_indices.clear();
        for (idx, entry) in self.events.iter().enumerate() {
            if self.entry_matches_filter(entry) {
                self.filtered_indices.push(idx);
            }
        }
        self.list_state.reset(self.filtered_indices.len());
    }

    pub fn set_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.search_query = query;
        self.recompute_filtered_indices();
        cx.notify();
    }

    fn clear_events(&mut self, cx: &mut Context<Self>) {
        self.events.clear();
        self.expanded.clear();
        self.filtered_indices.clear();
        self.list_state.reset(0);
        cx.notify();
    }

    fn show_read_error_toast(&self, error: &anyhow::Error, cx: &mut Context<Self>) {
        struct TelemetryLogReadError;
        cx.emit(TelemetryLogEvent::ShowToast(Toast::new(
            NotificationId::unique::<TelemetryLogReadError>(),
            format!("Failed to read telemetry log: {}", error),
        )));
    }

    fn show_parse_error_toast(&self, count: usize, cx: &mut Context<Self>) {
        struct TelemetryLogParseError;
        let message = if count == 1 {
            "1 telemetry log entry failed to parse".to_string()
        } else {
            format!("{} telemetry log entries failed to parse", count)
        };
        cx.emit(TelemetryLogEvent::ShowToast(Toast::new(
            NotificationId::unique::<TelemetryLogParseError>(),
            message,
        )));
    }

    fn render_entry(
        &mut self,
        filtered_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(&event_index) = self.filtered_indices.get(filtered_index) else {
            return Empty.into_any();
        };

        let Some(entry) = self.events.get(event_index) else {
            return Empty.into_any();
        };

        let base_size = TextSize::Editor.rems(cx);
        let text_style = window.text_style();
        let theme = cx.theme().clone();
        let colors = theme.colors();
        let border_color = colors.border;
        let element_background = colors.element_background;
        let selection_background_color = colors.element_selection_background;
        let syntax = theme.syntax().clone();
        let expanded = self.expanded.contains(&event_index);

        let local_timezone =
            time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
        let timestamp_str = time_format::format_localized_timestamp(
            entry.received_at,
            OffsetDateTime::now_utc(),
            local_timezone,
            time_format::TimestampFormat::EnhancedAbsolute,
        );

        let event_type = entry.event_type.clone();
        let signed_in = entry.signed_in;

        let collapsed_md = entry.collapsed_md.clone();

        let expanded_md =
            if expanded && entry.expanded_md.is_none() && !entry.event_properties.is_empty() {
                let language_registry = self.project.read(cx).languages().clone();
                let md = expanded_params_md(&entry.props_as_json_object(), &language_registry, cx);
                if let Some(entry_mut) = self.events.get_mut(event_index) {
                    entry_mut.expanded_md = Some(md.clone());
                }
                Some(md)
            } else if expanded {
                self.events
                    .get(event_index)
                    .and_then(|e| e.expanded_md.clone())
            } else {
                None
            };

        let params_md = if expanded { expanded_md } else { collapsed_md };

        let theme_settings = ThemeSettings::get_global(cx);
        let buffer_font_family = theme_settings.buffer_font.family.clone();

        v_flex()
            .id(filtered_index)
            .group("telemetry-entry")
            .cursor_pointer()
            .font_buffer(cx)
            .w_full()
            .py_3()
            .pl_4()
            .pr_5()
            .gap_2()
            .items_start()
            .text_size(base_size)
            .border_color(border_color)
            .border_b_1()
            .hover(|this| this.bg(element_background.opacity(0.5)))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.expanded.contains(&event_index) {
                    this.expanded.remove(&event_index);
                } else {
                    this.expanded.insert(event_index);
                    if let Some(filtered_idx) = this
                        .filtered_indices
                        .iter()
                        .position(|&idx| idx == event_index)
                    {
                        this.list_state.scroll_to_reveal_item(filtered_idx);
                    }
                }
                cx.notify()
            }))
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .flex_shrink_0()
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .color(Color::Muted)
                        .size(IconSize::Small),
                    )
                    .child(
                        Label::new(timestamp_str)
                            .buffer_font(cx)
                            .color(Color::Muted)
                            .size(LabelSize::Small),
                    )
                    .child(Label::new(event_type).buffer_font(cx).color(Color::Default))
                    .child(div().flex_1())
                    .when(signed_in, |this| {
                        this.child(
                            div()
                                .child(ui::Chip::new("signed in"))
                                .visible_on_hover("telemetry-entry"),
                        )
                    }),
            )
            .when_some(params_md, |this, params| {
                this.child(
                    div().pl_6().w_full().child(
                        MarkdownElement::new(
                            params,
                            MarkdownStyle {
                                base_text_style: text_style,
                                selection_background_color,
                                syntax: syntax.clone(),
                                code_block_overflow_x_scroll: expanded,
                                code_block: StyleRefinement {
                                    text: TextStyleRefinement {
                                        font_family: Some(buffer_font_family.clone()),
                                        font_size: Some((base_size * 0.8).into()),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        )
                        .code_block_renderer(CodeBlockRenderer::Default {
                            copy_button_visibility: if expanded {
                                CopyButtonVisibility::VisibleOnHover
                            } else {
                                CopyButtonVisibility::Hidden
                            },
                            wrap_button_visibility: WrapButtonVisibility::Hidden,
                            border: false,
                        }),
                    ),
                )
            })
            .into_any()
    }
}

pub enum TelemetryLogEvent {
    ShowToast(Toast),
}

impl EventEmitter<TelemetryLogEvent> for TelemetryLogView {}

impl Item for TelemetryLogView {
    type Event = TelemetryLogEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Telemetry Log".into()
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::Sparkle))
    }
}

impl Focusable for TelemetryLogView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TelemetryLogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(if self.filtered_indices.is_empty() {
                h_flex()
                    .size_full()
                    .justify_center()
                    .items_center()
                    .child(if self.events.is_empty() {
                        "No telemetry events recorded yet"
                    } else {
                        "No events match the current filter"
                    })
                    .into_any()
            } else {
                div()
                    .size_full()
                    .flex_grow_1()
                    .child(
                        list(self.list_state.clone(), cx.processor(Self::render_entry))
                            .with_sizing_behavior(gpui::ListSizingBehavior::Auto)
                            .size_full(),
                    )
                    .vertical_scrollbar_for(&self.list_state, window, cx)
                    .into_any()
            })
    }
}

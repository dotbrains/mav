use std::sync::Arc;

use collections::HashMap;
use gpui::{App, AppContext, Entity, SharedString};
use language::LanguageRegistry;
use markdown::Markdown;
use telemetry_events::{Event, EventWrapper};
use time::OffsetDateTime;

pub(super) struct TelemetryLogEntry {
    pub(super) received_at: OffsetDateTime,
    pub(super) event_type: SharedString,
    pub(super) event_properties: HashMap<String, serde_json::Value>,
    pub(super) signed_in: bool,
    pub(super) collapsed_md: Option<Entity<Markdown>>,
    pub(super) expanded_md: Option<Entity<Markdown>>,
}

impl TelemetryLogEntry {
    pub(super) fn props_as_json_object(&self) -> serde_json::Value {
        serde_json::Value::Object(
            self.event_properties
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }
}

pub(super) fn event_wrapper_to_entry(
    event_wrapper: &EventWrapper,
    language_registry: &Arc<LanguageRegistry>,
    cx: &mut App,
) -> TelemetryLogEntry {
    let (event_type, std_event_properties): (
        SharedString,
        std::collections::HashMap<String, serde_json::Value>,
    ) = match &event_wrapper.event {
        Event::Flexible(flexible) => (
            flexible.event_type.clone().into(),
            flexible.event_properties.clone(),
        ),
    };

    let event_properties: HashMap<String, serde_json::Value> =
        std_event_properties.into_iter().collect();

    let entry = TelemetryLogEntry {
        received_at: OffsetDateTime::now_utc(),
        event_type,
        event_properties,
        signed_in: event_wrapper.signed_in,
        collapsed_md: None,
        expanded_md: None,
    };

    let collapsed_md = if !entry.event_properties.is_empty() {
        Some(collapsed_params_md(
            &entry.props_as_json_object(),
            language_registry,
            cx,
        ))
    } else {
        None
    };

    TelemetryLogEntry {
        collapsed_md,
        ..entry
    }
}

fn collapsed_params_md(
    params: &serde_json::Value,
    language_registry: &Arc<LanguageRegistry>,
    cx: &mut App,
) -> Entity<Markdown> {
    let params_json = serde_json::to_string(params).unwrap_or_default();
    let mut spaced_out_json = String::with_capacity(params_json.len() + params_json.len() / 4);

    for ch in params_json.chars() {
        match ch {
            '{' => spaced_out_json.push_str("{ "),
            '}' => spaced_out_json.push_str(" }"),
            ':' => spaced_out_json.push_str(": "),
            ',' => spaced_out_json.push_str(", "),
            c => spaced_out_json.push(c),
        }
    }

    let params_md = format!("```json\n{}\n```", spaced_out_json);
    cx.new(|cx| Markdown::new(params_md.into(), Some(language_registry.clone()), None, cx))
}

pub(super) fn expanded_params_md(
    params: &serde_json::Value,
    language_registry: &Arc<LanguageRegistry>,
    cx: &mut App,
) -> Entity<Markdown> {
    let params_json = serde_json::to_string_pretty(params).unwrap_or_default();
    let params_md = format!("```json\n{}\n```", params_json);
    cx.new(|cx| Markdown::new(params_md.into(), Some(language_registry.clone()), None, cx))
}

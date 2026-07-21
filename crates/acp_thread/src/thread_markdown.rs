use super::*;

pub(super) fn markdown_for_raw_output(
    raw_output: &serde_json::Value,
    language_registry: &Arc<LanguageRegistry>,
    cx: &mut App,
) -> Option<Entity<Markdown>> {
    match raw_output {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(value) => Some(cx.new(|cx| {
            Markdown::new(
                value.to_string().into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
        serde_json::Value::Number(value) => Some(cx.new(|cx| {
            Markdown::new(
                value.to_string().into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
        serde_json::Value::String(value) => Some(cx.new(|cx| {
            Markdown::new(
                value.clone().into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
        value => Some(cx.new(|cx| {
            let pretty_json = to_string_pretty(value).unwrap_or_else(|_| value.to_string());

            Markdown::new(
                format!("```json\n{}\n```", pretty_json).into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
    }
}

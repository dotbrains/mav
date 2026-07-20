/// How a tool call is rendered relative to its surroundings.
///
/// `Standalone` draws its own border/margin/location header. `Embedded` is
/// hosted by a container that provides its own framing (e.g. the subagent
/// card). `Floating` is like `Embedded`, but used for the floating
/// awaiting-permission row above the message editor: the tool call's content
/// is height-capped and scrollable so the row can never grow to consume the
/// entire panel and squeeze the conversation list out of view.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum ToolCallLayout {
    Standalone,
    Embedded,
    Floating,
}

impl ToolCallLayout {
    /// Stable discriminant used to disambiguate element ids when the same tool
    /// call is rendered in more than one layout at once (e.g. inline in the
    /// list *and* in the floating awaiting-permission row).
    pub(super) fn id_str(self) -> &'static str {
        match self {
            ToolCallLayout::Standalone => "standalone",
            ToolCallLayout::Embedded => "embedded",
            ToolCallLayout::Floating => "floating",
        }
    }
}

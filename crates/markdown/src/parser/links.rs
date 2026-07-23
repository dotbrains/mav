use super::*;

pub fn parse_links_only(text: &str) -> Vec<(Range<usize>, MarkdownEvent)> {
    let mut events = Vec::new();
    let mut finder = LinkFinder::new();
    finder.kinds(&[linkify::LinkKind::Url]);
    let mut text_range = Range {
        start: 0,
        end: text.len(),
    };
    for link in finder.links(text) {
        let link_range = link.start()..link.end();

        if link_range.start > text_range.start {
            events.push((text_range.start..link_range.start, MarkdownEvent::Text));
        }

        events.push((
            link_range.clone(),
            MarkdownEvent::Start(MarkdownTag::Link {
                link_type: LinkType::Autolink,
                dest_url: SharedString::from(link.as_str().to_string()),
                title: SharedString::default(),
                id: SharedString::default(),
            }),
        ));
        events.push((link_range.clone(), MarkdownEvent::Text));
        events.push((link_range.clone(), MarkdownEvent::End(MarkdownTagEnd::Link)));

        text_range.start = link_range.end;
    }

    if text_range.end > text_range.start {
        events.push((text_range, MarkdownEvent::Text));
    }

    events
}

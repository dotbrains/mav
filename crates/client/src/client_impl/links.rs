use super::*;

/// prefix for the mav:// url scheme
pub const MAV_URL_SCHEME: &str = "mav";

/// A parsed Mav link that can be handled internally by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MavLink {
    /// Join a channel: `mav.dev/channel/channel-name-123` or `mav://channel/channel-name-123`
    Channel { channel_id: u64 },
    /// Open channel notes: `mav.dev/channel/channel-name-123/notes` or with heading `notes#heading`
    ChannelNotes {
        channel_id: u64,
        heading: Option<String>,
    },
}

/// Parses the given link into a Mav link.
///
/// Returns a [`Some`] containing the parsed link if the link is a recognized Mav link
/// that should be handled internally by the application.
/// Returns [`None`] for links that should be opened in the browser.
pub fn parse_mav_link(link: &str, cx: &App) -> Option<MavLink> {
    let server_url = &ClientSettings::get_global(cx).server_url;
    let path = link
        .strip_prefix(server_url)
        .and_then(|result| result.strip_prefix('/'))
        .or_else(|| {
            link.strip_prefix(MAV_URL_SCHEME)
                .and_then(|result| result.strip_prefix("://"))
        })?;

    let mut parts = path.split('/');

    if parts.next() != Some("channel") {
        return None;
    }

    let slug = parts.next()?;
    let id_str = slug.split('-').next_back()?;
    let channel_id = id_str.parse::<u64>().ok()?;

    let Some(next) = parts.next() else {
        return Some(MavLink::Channel { channel_id });
    };

    if let Some(heading) = next.strip_prefix("notes#") {
        return Some(MavLink::ChannelNotes {
            channel_id,
            heading: Some(heading.to_string()),
        });
    }

    if next == "notes" {
        return Some(MavLink::ChannelNotes {
            channel_id,
            heading: None,
        });
    }

    None
}

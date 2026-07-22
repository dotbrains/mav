use super::*;

const SKILL_SHARE_LINK_SCHEME: &str = "mav";
/// The host (the part after `mav://`) that identifies a skill share link.
const SKILL_SHARE_LINK_HOST: &str = "skill";
/// The query parameter that carries the embedded `SKILL.md` payload.
const SKILL_SHARE_LINK_DATA_PARAM: &str = "data";

/// The `mav://` deep-link prefix for a shared skill. Opening a link with this
/// prefix prompts the recipient to review and install the embedded skill.
pub const SKILL_SHARE_LINK_PREFIX: &str =
    concatcp!(SKILL_SHARE_LINK_SCHEME, "://", SKILL_SHARE_LINK_HOST);

/// Build a shareable `mav://skill?data=…` link that fully embeds the given
/// `SKILL.md` file contents.
///
/// The contents are base64url-encoded (no padding) so the link is
/// self-contained and URL-safe: the recipient doesn't need the skill to be
/// hosted anywhere. Recover the contents with [`decode_skill_share_link`].
pub fn encode_skill_share_link(skill_file_content: &str) -> String {
    use base64::Engine as _;
    let data =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(skill_file_content.as_bytes());
    let mut url = Url::parse(SKILL_SHARE_LINK_PREFIX).expect("skill share link prefix is valid");
    url.query_pairs_mut()
        .append_pair(SKILL_SHARE_LINK_DATA_PARAM, &data);
    url.into()
}

/// Recover the `SKILL.md` contents embedded in a `mav://skill?data=…` link
/// produced by [`encode_skill_share_link`].
pub fn decode_skill_share_link(link: &str) -> Result<String> {
    use base64::Engine as _;
    let url = Url::parse(link).context("skill share link is not a valid URL")?;
    anyhow::ensure!(
        url.scheme() == SKILL_SHARE_LINK_SCHEME && url.host_str() == Some(SKILL_SHARE_LINK_HOST),
        "not a skill share link"
    );
    let data = url
        .query_pairs()
        .find_map(|(key, value)| (key == SKILL_SHARE_LINK_DATA_PARAM).then_some(value))
        .context("skill share link is missing the `data` parameter")?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data.as_bytes())
        .context("skill share link `data` is not valid base64")?;
    anyhow::ensure!(
        bytes.len() <= MAX_SKILL_FILE_SIZE,
        "shared skill exceeds the maximum size of {MAX_SKILL_FILE_SIZE} bytes"
    );
    let content = String::from_utf8(bytes).context("skill share link `data` is not valid UTF-8")?;
    Ok(content)
}

use super::*;

#[test]
fn slugify_output_always_passes_validate_name() {
    for input in [
        "foo",
        "Foo Bar",
        "rock & roll",
        "---weird---",
        "a".repeat(200).as_str(),
    ] {
        if let Some(slug) = slugify_skill_name(input) {
            assert!(
                validate_name(&slug).is_ok(),
                "slug {slug:?} from {input:?} failed validate_name"
            );
        }
    }
}

#[test]
fn skill_share_link_round_trips() {
    let content =
        "---\nname: my-skill\ndescription: Does a thing.\n---\n\n## Steps\n\nDo the thing.\n";
    let link = encode_skill_share_link(content);
    let data = link
        .strip_prefix("mav://skill?data=")
        .expect("link should start with the skill share prefix");
    // base64url (no-pad) output must not require percent-encoding.
    assert!(!data.contains('+') && !data.contains('/') && !data.contains('='));
    assert_eq!(decode_skill_share_link(&link).unwrap(), content);
}

#[test]
fn decode_skill_share_link_rejects_non_skill_links() {
    assert!(decode_skill_share_link("mav://settings/agent.skills").is_err());
    assert!(decode_skill_share_link("mav://skill").is_err());
    assert!(decode_skill_share_link("mav://skill?other=1").is_err());
    assert!(decode_skill_share_link("mav://skill?data=!!!notbase64").is_err());
}

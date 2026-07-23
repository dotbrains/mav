use super::MarkdownEvent::*;
use super::MarkdownTag::*;
use super::*;

const CONDITIONAL_OPTIONS: Options = Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
const UNWANTED_OPTIONS: Options = Options::ENABLE_MATH
    .union(Options::ENABLE_DEFINITION_LIST)
    .union(Options::ENABLE_WIKILINKS);

#[test]
fn all_options_considered() {
    // The purpose of this is to fail when new options are added to pulldown_cmark, so that they
    // can be evaluated for inclusion.
    assert_eq!(
        PARSE_OPTIONS
            .union(CONDITIONAL_OPTIONS)
            .union(UNWANTED_OPTIONS),
        Options::all()
    );
}

#[test]
fn wanted_and_unwanted_options_disjoint() {
    assert_eq!(
        PARSE_OPTIONS
            .union(CONDITIONAL_OPTIONS)
            .intersection(UNWANTED_OPTIONS),
        Options::empty()
    );
}

#[test]
fn test_yaml_style_metadata_block() {
    assert_eq!(
        parse_markdown_with_options("---\ntitle: Post\n---\n# Heading", false, false, true),
        ParsedMarkdownData {
            events: vec![
                (0..19, RootStart),
                (0..19, Start(MetadataBlock(MetadataBlockKind::YamlStyle))),
                (4..16, Text),
                (
                    0..19,
                    End(MarkdownTagEnd::MetadataBlock(MetadataBlockKind::YamlStyle))
                ),
                (0..19, RootEnd(0)),
                (20..29, RootStart),
                (
                    20..29,
                    Start(Heading {
                        level: HeadingLevel::H1,
                        id: None,
                        classes: Vec::new(),
                        attrs: Vec::new(),
                    })
                ),
                (22..29, Text),
                (20..29, End(MarkdownTagEnd::Heading(HeadingLevel::H1))),
                (20..29, RootEnd(1)),
            ],
            root_block_starts: vec![0, 20],
            metadata_blocks: BTreeMap::from_iter([(
                0,
                ParsedMetadataBlock {
                    content_range: 4..16,
                    rows: Some(vec![MetadataRow {
                        key: 4..9,
                        value: 11..15,
                    }]),
                },
            )]),
            ..Default::default()
        }
    )
}

#[test]
fn test_metadata_block_text_is_verbatim() {
    let parsed =
        parse_markdown_with_options("---\nurl: https://mav.dev\n---\nBody", false, false, true);
    assert!(
        parsed
            .events
            .iter()
            .all(|(_, event)| !matches!(event, Start(Link { .. })))
    );
}

#[test]
fn test_metadata_blocks_store_table_rows() {
    let parsed = parse_markdown_with_options(
        "---\ntitle: Post\nauthor: Mav\n---\nBody",
        false,
        false,
        true,
    );

    assert_eq!(
        parsed.metadata_blocks,
        BTreeMap::from_iter([(
            0,
            ParsedMetadataBlock {
                content_range: 4..28,
                rows: Some(vec![
                    MetadataRow {
                        key: 4..9,
                        value: 11..15,
                    },
                    MetadataRow {
                        key: 16..22,
                        value: 24..27,
                    },
                ]),
            },
        )])
    );
}

#[test]
fn test_metadata_blocks_store_fallback_for_nested_yaml() {
    let parsed = parse_markdown_with_options("---\ntags:\n  - mav\n---\nBody", false, false, true);

    assert_eq!(
        parsed.metadata_blocks,
        BTreeMap::from_iter([(
            0,
            ParsedMetadataBlock {
                content_range: 4..18,
                rows: None,
            },
        )])
    );
}

#[test]
fn test_metadata_table_rows_parse_simple_colon_pairs() {
    let source = "title: Post\nauthor: Mav\n";
    let Some(rows) = parse_metadata_table_rows(source, 0..source.len()) else {
        panic!("expected metadata rows");
    };
    let pairs = rows
        .into_iter()
        .map(|row| (&source[row.key], &source[row.value]))
        .collect::<Vec<_>>();

    assert_eq!(pairs, vec![("title", "Post"), ("author", "Mav")]);
}

#[test]
fn test_metadata_table_rows_reject_non_simple_colon_pairs() {
    for source in [
        "tags:\n  - mav\n",
        "title = Post\n",
        "title:\n",
        "title:   \n",
        ": Post\n",
        " title: Post\n",
        "\n",
    ] {
        assert!(parse_metadata_table_rows(source, 0..source.len()).is_none());
    }
}

#[test]
fn test_trim_metadata_range_returns_valid_empty_range() {
    let source = "key:   \n";
    let trimmed = trim_metadata_range(source, 4..7);

    assert_eq!(trimmed, 7..7);
    assert!(source[trimmed].is_empty());
}

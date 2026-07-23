use super::*;

#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ParsedMarkdownData {
    pub events: Vec<(Range<usize>, MarkdownEvent)>,
    pub language_names: HashSet<SharedString>,
    pub language_paths: HashSet<Arc<str>>,
    pub root_block_starts: Vec<usize>,
    pub html_blocks: BTreeMap<usize, html::html_parser::ParsedHtmlBlock>,
    pub metadata_blocks: BTreeMap<usize, ParsedMetadataBlock>,
    pub heading_slugs: HashMap<SharedString, usize>,
    pub footnote_definitions: HashMap<SharedString, usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedMetadataBlock {
    pub content_range: Range<usize>,
    pub rows: Option<Vec<MetadataRow>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MetadataRow {
    pub key: Range<usize>,
    pub value: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MarkdownEvent {
    /// Start of a tagged element. Events that are yielded after this event
    /// and before its corresponding `End` event are inside this element.
    /// Start and end events are guaranteed to be balanced.
    Start(MarkdownTag),
    /// End of a tagged element.
    End(MarkdownTagEnd),
    /// Text that uses the associated range from the markdown source.
    Text,
    /// Text that differs from the markdown source - typically due to substitution of HTML entities
    /// and smart punctuation.
    SubstitutedText(String),
    /// An inline code node.
    Code,
    /// An inline code node that differs from the markdown source due to escape decoding.
    SubstitutedCode(String),
    /// An HTML node.
    Html,
    /// An inline HTML node.
    InlineHtml,
    /// A reference to a footnote with given label, which may or may not be defined
    /// by an event with a `Tag::FootnoteDefinition` tag. Definitions and references to them may
    /// occur in any order.
    FootnoteReference(SharedString),
    /// A soft line break.
    SoftBreak,
    /// A hard line break.
    HardBreak,
    /// A horizontal ruler.
    Rule,
    /// A task list marker, rendered as a checkbox in HTML. Contains a true when it is checked.
    TaskListMarker(bool),
    /// Start of a root-level block (a top-level structural element like a paragraph, heading, list, etc.).
    RootStart,
    /// End of a root-level block. Contains the root block index.
    RootEnd(usize),
}

/// Tags for elements that can contain other elements.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkdownTag {
    /// A paragraph of text and other inline elements.
    Paragraph,

    /// A heading, with optional identifier, classes and custom attributes.
    /// The identifier is prefixed with `#` and the last one in the attributes
    /// list is chosen, classes are prefixed with `.` and custom attributes
    /// have no prefix and can optionally have a value (`myattr` o `myattr=myvalue`).
    Heading {
        level: HeadingLevel,
        id: Option<SharedString>,
        classes: Vec<SharedString>,
        /// The first item of the tuple is the attr and second one the value.
        attrs: Vec<(SharedString, Option<SharedString>)>,
    },

    BlockQuote(Option<pulldown_cmark::BlockQuoteKind>),

    /// A code block.
    CodeBlock {
        kind: CodeBlockKind,
        metadata: CodeBlockMetadata,
    },

    /// A HTML block.
    HtmlBlock,

    /// A list. If the list is ordered the field indicates the number of the first item.
    /// Contains only list items.
    List(Option<u64>), // TODO: add delim and tight for ast (not needed for html)

    /// A list item.
    Item,

    /// A footnote definition. The value contained is the footnote's label by which it can
    /// be referred to.
    FootnoteDefinition(SharedString),

    /// A table. Contains a vector describing the text-alignment for each of its columns.
    Table(Vec<Alignment>),

    /// A table header. Contains only `TableCell`s. Note that the table body starts immediately
    /// after the closure of the `TableHead` tag. There is no `TableBody` tag.
    TableHead,

    /// A table row. Is used both for header rows as body rows. Contains only `TableCell`s.
    TableRow,
    TableCell,

    // span-level tags
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,

    /// A link.
    Link {
        link_type: LinkType,
        dest_url: SharedString,
        title: SharedString,
        /// Identifier of reference links, e.g. `world` in the link `[hello][world]`.
        id: SharedString,
    },

    /// An image. The first field is the link type, the second the destination URL and the third is a title,
    /// the fourth is the link identifier.
    Image {
        link_type: LinkType,
        dest_url: SharedString,
        title: SharedString,
        /// Identifier of reference links, e.g. `world` in the link `[hello][world]`.
        id: SharedString,
    },

    /// A metadata block.
    MetadataBlock(MetadataBlockKind),

    DefinitionList,
    DefinitionListTitle,
    DefinitionListDefinition,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CodeBlockKind {
    Indented,
    /// "Fenced" means "surrounded by triple backticks."
    /// There can optionally be either a language after the backticks (like in traditional Markdown)
    /// or, if an agent is specifying a path for a source location in the project, it can be a PathRange,
    /// e.g. ```path/to/foo.rs#L123-456 instead of ```rust
    Fenced,
    FencedLang(SharedString),
    FencedSrc(PathWithRange),
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CodeBlockMetadata {
    pub content_range: Range<usize>,
    pub line_count: usize,
    pub is_fenced_closed: bool,
}

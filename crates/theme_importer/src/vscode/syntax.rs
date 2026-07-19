use indexmap::IndexMap;
use serde::Deserialize;
use strum::EnumIter;

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum VsCodeTokenScope {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct VsCodeTokenColor {
    pub name: Option<String>,
    pub scope: Option<VsCodeTokenScope>,
    pub settings: VsCodeTokenColorSettings,
}

#[derive(Debug, Deserialize)]
pub struct VsCodeTokenColorSettings {
    pub foreground: Option<String>,
    pub background: Option<String>,
    #[serde(rename = "fontStyle")]
    pub font_style: Option<String>,
}

#[derive(Debug, PartialEq, Copy, Clone, EnumIter)]
pub enum MavSyntaxToken {
    Attribute,
    Boolean,
    Comment,
    CommentDoc,
    Constant,
    Constructor,
    Embedded,
    Emphasis,
    EmphasisStrong,
    Enum,
    Function,
    Hint,
    Keyword,
    Label,
    LinkText,
    LinkUri,
    Number,
    Operator,
    Predictive,
    Preproc,
    Primary,
    Property,
    Punctuation,
    PunctuationBracket,
    PunctuationDelimiter,
    PunctuationListMarker,
    PunctuationSpecial,
    String,
    StringEscape,
    StringRegex,
    StringSpecial,
    StringSpecialSymbol,
    Tag,
    TextLiteral,
    Title,
    Type,
    Variable,
    VariableSpecial,
    Variant,
}

impl std::fmt::Display for MavSyntaxToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                MavSyntaxToken::Attribute => "attribute",
                MavSyntaxToken::Boolean => "boolean",
                MavSyntaxToken::Comment => "comment",
                MavSyntaxToken::CommentDoc => "comment.doc",
                MavSyntaxToken::Constant => "constant",
                MavSyntaxToken::Constructor => "constructor",
                MavSyntaxToken::Embedded => "embedded",
                MavSyntaxToken::Emphasis => "emphasis",
                MavSyntaxToken::EmphasisStrong => "emphasis.strong",
                MavSyntaxToken::Enum => "enum",
                MavSyntaxToken::Function => "function",
                MavSyntaxToken::Hint => "hint",
                MavSyntaxToken::Keyword => "keyword",
                MavSyntaxToken::Label => "label",
                MavSyntaxToken::LinkText => "link_text",
                MavSyntaxToken::LinkUri => "link_uri",
                MavSyntaxToken::Number => "number",
                MavSyntaxToken::Operator => "operator",
                MavSyntaxToken::Predictive => "predictive",
                MavSyntaxToken::Preproc => "preproc",
                MavSyntaxToken::Primary => "primary",
                MavSyntaxToken::Property => "property",
                MavSyntaxToken::Punctuation => "punctuation",
                MavSyntaxToken::PunctuationBracket => "punctuation.bracket",
                MavSyntaxToken::PunctuationDelimiter => "punctuation.delimiter",
                MavSyntaxToken::PunctuationListMarker => "punctuation.list_marker",
                MavSyntaxToken::PunctuationSpecial => "punctuation.special",
                MavSyntaxToken::String => "string",
                MavSyntaxToken::StringEscape => "string.escape",
                MavSyntaxToken::StringRegex => "string.regex",
                MavSyntaxToken::StringSpecial => "string.special",
                MavSyntaxToken::StringSpecialSymbol => "string.special.symbol",
                MavSyntaxToken::Tag => "tag",
                MavSyntaxToken::TextLiteral => "text.literal",
                MavSyntaxToken::Title => "title",
                MavSyntaxToken::Type => "type",
                MavSyntaxToken::Variable => "variable",
                MavSyntaxToken::VariableSpecial => "variable.special",
                MavSyntaxToken::Variant => "variant",
            }
        )
    }
}

impl MavSyntaxToken {
    pub fn find_best_token_color_match<'a>(
        &self,
        token_colors: &'a [VsCodeTokenColor],
    ) -> Option<&'a VsCodeTokenColor> {
        let mut ranked_matches = IndexMap::new();

        for (ix, token_color) in token_colors.iter().enumerate() {
            if token_color.settings.foreground.is_none() {
                continue;
            }

            let Some(rank) = self.rank_match(token_color) else {
                continue;
            };

            if rank > 0 {
                ranked_matches.insert(ix, rank);
            }
        }

        ranked_matches
            .into_iter()
            .max_by_key(|(_, rank)| *rank)
            .map(|(ix, _)| &token_colors[ix])
    }

    fn rank_match(&self, token_color: &VsCodeTokenColor) -> Option<u32> {
        let candidate_scopes = match token_color.scope.as_ref()? {
            VsCodeTokenScope::One(scope) => vec![scope],
            VsCodeTokenScope::Many(scopes) => scopes.iter().collect(),
        }
        .iter()
        .flat_map(|scope| scope.split(',').map(|s| s.trim()))
        .collect::<Vec<_>>();

        let scopes_to_match = self.to_vscode();
        let number_of_scopes_to_match = scopes_to_match.len();

        let mut matches = 0;

        for (ix, scope) in scopes_to_match.into_iter().enumerate() {
            // Assign each entry a weight that is inversely proportional to its
            // position in the list.
            //
            // Entries towards the front are weighted higher than those towards the end.
            let weight = (number_of_scopes_to_match - ix) as u32;

            if candidate_scopes.contains(&scope) {
                matches += 1 + weight;
            }
        }

        Some(matches)
    }

    pub fn fallbacks(&self) -> &[Self] {
        match self {
            MavSyntaxToken::CommentDoc => &[MavSyntaxToken::Comment],
            MavSyntaxToken::Number => &[MavSyntaxToken::Constant],
            MavSyntaxToken::VariableSpecial => &[MavSyntaxToken::Variable],
            MavSyntaxToken::PunctuationBracket
            | MavSyntaxToken::PunctuationDelimiter
            | MavSyntaxToken::PunctuationListMarker
            | MavSyntaxToken::PunctuationSpecial => &[MavSyntaxToken::Punctuation],
            MavSyntaxToken::StringEscape
            | MavSyntaxToken::StringRegex
            | MavSyntaxToken::StringSpecial
            | MavSyntaxToken::StringSpecialSymbol => &[MavSyntaxToken::String],
            _ => &[],
        }
    }

    fn to_vscode(self) -> Vec<&'static str> {
        match self {
            MavSyntaxToken::Attribute => vec!["entity.other.attribute-name"],
            MavSyntaxToken::Boolean => vec!["constant.language"],
            MavSyntaxToken::Comment => vec!["comment"],
            MavSyntaxToken::CommentDoc => vec!["comment.block.documentation"],
            MavSyntaxToken::Constant => vec!["constant", "constant.language", "constant.character"],
            MavSyntaxToken::Constructor => {
                vec![
                    "entity.name.tag",
                    "entity.name.function.definition.special.constructor",
                ]
            }
            MavSyntaxToken::Embedded => vec!["meta.embedded"],
            MavSyntaxToken::Emphasis => vec!["markup.italic"],
            MavSyntaxToken::EmphasisStrong => vec![
                "markup.bold",
                "markup.italic markup.bold",
                "markup.bold markup.italic",
            ],
            MavSyntaxToken::Enum => vec!["support.type.enum"],
            MavSyntaxToken::Function => vec![
                "entity.function",
                "entity.name.function",
                "variable.function",
            ],
            MavSyntaxToken::Hint => vec![],
            MavSyntaxToken::Keyword => vec![
                "keyword",
                "keyword.other.fn.rust",
                "keyword.control",
                "keyword.control.fun",
                "keyword.control.class",
                "punctuation.accessor",
                "entity.name.tag",
            ],
            MavSyntaxToken::Label => vec![
                "label",
                "entity.name",
                "entity.name.import",
                "entity.name.package",
            ],
            MavSyntaxToken::LinkText => vec!["markup.underline.link", "string.other.link"],
            MavSyntaxToken::LinkUri => vec!["markup.underline.link", "string.other.link"],
            MavSyntaxToken::Number => vec!["constant.numeric", "number"],
            MavSyntaxToken::Operator => vec!["operator", "keyword.operator"],
            MavSyntaxToken::Predictive => vec![],
            MavSyntaxToken::Preproc => vec![
                "preproc",
                "meta.preprocessor",
                "punctuation.definition.preprocessor",
            ],
            MavSyntaxToken::Primary => vec![],
            MavSyntaxToken::Property => vec![
                "variable.member",
                "support.type.property-name",
                "variable.object.property",
                "variable.other.field",
            ],
            MavSyntaxToken::Punctuation => vec![
                "punctuation",
                "punctuation.section",
                "punctuation.accessor",
                "punctuation.separator",
                "punctuation.definition.tag",
            ],
            MavSyntaxToken::PunctuationBracket => vec![
                "punctuation.bracket",
                "punctuation.definition.tag.begin",
                "punctuation.definition.tag.end",
            ],
            MavSyntaxToken::PunctuationDelimiter => vec![
                "punctuation.delimiter",
                "punctuation.separator",
                "punctuation.terminator",
            ],
            MavSyntaxToken::PunctuationListMarker => {
                vec!["markup.list punctuation.definition.list.begin"]
            }
            MavSyntaxToken::PunctuationSpecial => vec!["punctuation.special"],
            MavSyntaxToken::String => vec!["string"],
            MavSyntaxToken::StringEscape => {
                vec!["string.escape", "constant.character", "constant.other"]
            }
            MavSyntaxToken::StringRegex => vec!["string.regex"],
            MavSyntaxToken::StringSpecial => vec!["string.special", "constant.other.symbol"],
            MavSyntaxToken::StringSpecialSymbol => {
                vec!["string.special.symbol", "constant.other.symbol"]
            }
            MavSyntaxToken::Tag => vec!["tag", "entity.name.tag", "meta.tag.sgml"],
            MavSyntaxToken::TextLiteral => vec!["text.literal", "string"],
            MavSyntaxToken::Title => vec!["title", "entity.name"],
            MavSyntaxToken::Type => vec![
                "entity.name.type",
                "entity.name.type.primitive",
                "entity.name.type.numeric",
                "keyword.type",
                "support.type",
                "support.type.primitive",
                "support.class",
            ],
            MavSyntaxToken::Variable => vec![
                "variable",
                "variable.language",
                "variable.member",
                "variable.parameter",
                "variable.parameter.function-call",
            ],
            MavSyntaxToken::VariableSpecial => vec![
                "variable.special",
                "variable.member",
                "variable.annotation",
                "variable.language",
            ],
            MavSyntaxToken::Variant => vec!["variant"],
        }
    }
}

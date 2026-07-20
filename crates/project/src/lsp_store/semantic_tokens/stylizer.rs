use collections::HashMap;
use gpui::{App, SharedString};
use language::LanguageServerId;
use settings::{DefaultSemanticTokenRules, SemanticTokenRule, SemanticTokenRules, Settings as _};

use crate::project_settings::ProjectSettings;

use super::TokenType;

pub struct SemanticTokenStylizer {
    server_id: LanguageServerId,
    rules_by_token_type: HashMap<TokenType, Vec<SemanticTokenRule>>,
    token_type_names: HashMap<TokenType, SharedString>,
    modifier_mask: HashMap<SharedString, u32>,
}

impl SemanticTokenStylizer {
    pub fn new(
        server_id: LanguageServerId,
        legend: &lsp::SemanticTokensLegend,
        language_rules: Option<&SemanticTokenRules>,
        cx: &App,
    ) -> Self {
        let token_types: HashMap<TokenType, SharedString> = legend
            .token_types
            .iter()
            .enumerate()
            .map(|(i, token_type)| {
                (
                    TokenType(i as u32),
                    SharedString::from(token_type.as_str().to_string()),
                )
            })
            .collect();
        let modifier_mask: HashMap<SharedString, u32> = legend
            .token_modifiers
            .iter()
            .enumerate()
            .map(|(i, modifier)| (SharedString::from(modifier.as_str().to_string()), 1 << i))
            .collect();

        let global_rules = &ProjectSettings::get_global(cx)
            .global_lsp_settings
            .semantic_token_rules;
        let default_rules = cx.global::<DefaultSemanticTokenRules>();

        let rules_by_token_type = token_types
            .iter()
            .map(|(index, token_type_name)| {
                let filter = |rule: &&SemanticTokenRule| {
                    rule.token_type
                        .as_ref()
                        .is_none_or(|rule_token_type| rule_token_type == token_type_name.as_ref())
                };
                let matching_rules: Vec<SemanticTokenRule> = global_rules
                    .rules
                    .iter()
                    .chain(language_rules.into_iter().flat_map(|lr| &lr.rules))
                    .chain(default_rules.0.rules.iter())
                    .rev()
                    .filter(filter)
                    .cloned()
                    .collect();
                (*index, matching_rules)
            })
            .collect();

        SemanticTokenStylizer {
            server_id,
            rules_by_token_type,
            token_type_names: token_types,
            modifier_mask,
        }
    }

    pub fn server_id(&self) -> LanguageServerId {
        self.server_id
    }

    pub fn token_type_name(&self, token_type: TokenType) -> Option<&SharedString> {
        self.token_type_names.get(&token_type)
    }

    pub fn has_modifier(&self, token_modifiers: u32, modifier: &str) -> bool {
        let Some(mask) = self.modifier_mask.get(modifier) else {
            return false;
        };
        (token_modifiers & mask) != 0
    }

    pub fn token_modifiers(&self, token_modifiers: u32) -> Option<String> {
        let modifiers: Vec<&str> = self
            .modifier_mask
            .iter()
            .filter(|(_, mask)| (token_modifiers & *mask) != 0)
            .map(|(name, _)| name.as_ref())
            .collect();
        if modifiers.is_empty() {
            None
        } else {
            Some(modifiers.join(", "))
        }
    }

    pub fn rules_for_token(&self, token_type: TokenType) -> Option<&[SemanticTokenRule]> {
        self.rules_by_token_type
            .get(&token_type)
            .map(|v| v.as_slice())
    }
}

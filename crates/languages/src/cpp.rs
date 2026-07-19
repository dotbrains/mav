use settings::SemanticTokenRules;

pub(crate) fn semantic_token_rules() -> SemanticTokenRules {
    let content = grammars::get_file("cpp/semantic_token_rules.json")
        .expect("missing cpp/semantic_token_rules.json");
    let json = std::str::from_utf8(&content.data).expect("invalid utf-8 in semantic_token_rules");
    settings::parse_json_with_comments::<SemanticTokenRules>(json)
        .expect("failed to parse cpp semantic_token_rules.json")
}

#[cfg(test)]
#[path = "cpp/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cpp/if_else_tests.rs"]
mod if_else_tests;

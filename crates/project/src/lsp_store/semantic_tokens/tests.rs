use super::*;
use crate::lsp_command::SemanticTokensEdit;
use lsp::SEMANTIC_TOKEN_MODIFIERS;

fn modifier_names(bits: u32) -> String {
    if bits == 0 {
        return "-".to_string();
    }
    let names: Vec<&str> = SEMANTIC_TOKEN_MODIFIERS
        .iter()
        .enumerate()
        .filter(|(i, _)| bits & (1 << i) != 0)
        .map(|(_, m)| m.as_str())
        .collect();

    let known_bits = (1u32 << SEMANTIC_TOKEN_MODIFIERS.len()) - 1;
    let unknown = bits & !known_bits;

    if unknown != 0 {
        let mut result = names.join("+");
        if !result.is_empty() {
            result.push('+');
        }
        result.push_str(&format!("?0x{:x}", unknown));
        result
    } else {
        names.join("+")
    }
}

/// Debug tool: parses semantic token JSON from LSP and prints human-readable output.
///
/// Usage: Paste JSON into `json_input`, then run:
///   cargo test -p project debug_parse_tokens -- --nocapture --ignored
///
/// Accepts either:
/// - Full LSP response: `{"jsonrpc":"2.0","id":1,"result":{"data":[...]}}`
/// - Just the data array: `[0,0,5,1,0,...]`
///
/// For delta responses, paste multiple JSON messages (one per line) and they
/// will be applied in sequence.
///
/// Token encoding (5 values per token):
///   [deltaLine, deltaStart, length, tokenType, tokenModifiers]
#[test]
#[ignore] // Run with: cargo test -p project debug_parse_tokens -- --nocapture --ignored
fn debug_parse_tokens() {
    let json_input = r#"
// === EXAMPLE 1: Full response (LSP spec example) ===
// 3 tokens: property at line 2, type at line 2, class at line 5
{"jsonrpc":"2.0","id":1,"result":{"resultId":"1","data":[2,5,3,9,3,0,5,4,6,0,3,2,7,1,0]}}

// === EXAMPLE 2: Delta response ===
// User added empty line at start of file, so all tokens shift down by 1 line.
// This changes first token's deltaLine from 2 to 3 (edit at index 0).
{"jsonrpc":"2.0","id":2,"result":{"resultId":"2","edits":[{"start":0,"deleteCount":1,"data":[3]}]}}

// === EXAMPLE 3: Another delta ===
// User added a new token. Insert 5 values at position 5 (after first token).
// New token: same line as token 1, 2 chars after it ends, len 5, type=function(12), mods=definition(2)
{"jsonrpc":"2.0","id":3,"result":{"resultId":"3","edits":[{"start":5,"deleteCount":0,"data":[0,2,5,12,2]}]}}
        "#;

    let mut current_data: Vec<u32> = Vec::new();
    let mut result_id: Option<String> = None;

    for line in json_input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        let parsed: serde_json::Value = serde_json::from_str(line).expect("Failed to parse JSON");
        let (data, edits, new_result_id) = extract_semantic_tokens(&parsed);

        if let Some(new_id) = new_result_id {
            result_id = Some(new_id);
        }

        if let Some(full_data) = data {
            println!("\n{}", "=".repeat(70));
            println!("FULL RESPONSE (resultId: {:?})", result_id);
            current_data = full_data;
        } else if let Some(delta_edits) = edits {
            println!("\n{}", "=".repeat(70));
            println!(
                "DELTA RESPONSE: {} edit(s) (resultId: {:?})",
                delta_edits.len(),
                result_id
            );
            for (i, edit) in delta_edits.iter().enumerate() {
                println!(
                    "  [{}] start={}, delete={}, insert {} values",
                    i,
                    edit.start,
                    edit.delete_count,
                    edit.data.len()
                );
            }
            let mut tokens = ServerSemanticTokens::from_full(current_data.clone(), None);
            tokens.apply(&delta_edits);
            current_data = tokens.data;
        }
    }

    println!(
        "\nDATA: {} values = {} tokens",
        current_data.len(),
        current_data.len() / 5
    );
    println!("\nPARSED TOKENS:");
    println!("{:-<100}", "");
    println!(
        "{:>5} {:>6} {:>4}  {:<15} {}",
        "LINE", "START", "LEN", "TYPE", "MODIFIERS"
    );
    println!("{:-<100}", "");

    let tokens = ServerSemanticTokens::from_full(current_data, None);
    for token in tokens.tokens() {
        println!(
            "{:>5} {:>6} {:>4}  {:<15} {}",
            token.line,
            token.start,
            token.length,
            token.token_type.0,
            modifier_names(token.token_modifiers),
        );
    }
    println!("{:-<100}", "");
    println!("{}\n", "=".repeat(100));
}

fn extract_semantic_tokens(
    value: &serde_json::Value,
) -> (
    Option<Vec<u32>>,
    Option<Vec<SemanticTokensEdit>>,
    Option<String>,
) {
    if let Some(arr) = value.as_array() {
        let data: Vec<u32> = arr
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u32))
            .collect();
        return (Some(data), None, None);
    }

    let result = value.get("result").unwrap_or(value);
    let result_id = result
        .get("resultId")
        .and_then(|v| v.as_str())
        .map(String::from);

    if let Some(data_arr) = result.get("data").and_then(|v| v.as_array()) {
        let data: Vec<u32> = data_arr
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u32))
            .collect();
        return (Some(data), None, result_id);
    }

    if let Some(edits_arr) = result.get("edits").and_then(|v| v.as_array()) {
        let edits: Vec<SemanticTokensEdit> = edits_arr
            .iter()
            .filter_map(|e| {
                Some(SemanticTokensEdit {
                    start: e.get("start")?.as_u64()? as u32,
                    delete_count: e.get("deleteCount")?.as_u64()? as u32,
                    data: e
                        .get("data")
                        .and_then(|d| d.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u32))
                                .collect()
                        })
                        .unwrap_or_default(),
                })
            })
            .collect();
        return (None, Some(edits), result_id);
    }

    (None, None, result_id)
}

#[test]
fn parses_sample_tokens() {
    let tokens =
        ServerSemanticTokens::from_full(vec![2, 5, 3, 0, 3, 0, 5, 4, 1, 0, 3, 2, 7, 2, 0], None)
            .tokens()
            .collect::<Vec<SemanticToken>>();

    assert_eq!(
        tokens,
        &[
            SemanticToken {
                line: 2,
                start: 5,
                length: 3,
                token_type: TokenType(0),
                token_modifiers: 3
            },
            SemanticToken {
                line: 2,
                start: 10,
                length: 4,
                token_type: TokenType(1),
                token_modifiers: 0
            },
            SemanticToken {
                line: 5,
                start: 2,
                length: 7,
                token_type: TokenType(2),
                token_modifiers: 0
            }
        ]
    );
}

#[test]
fn applies_delta_edit() {
    let mut tokens =
        ServerSemanticTokens::from_full(vec![2, 5, 3, 0, 3, 0, 5, 4, 1, 0, 3, 2, 7, 2, 0], None);

    tokens.apply(&[SemanticTokensEdit {
        start: 0,
        delete_count: 1,
        data: vec![3],
    }]);

    let result = tokens.tokens().collect::<Vec<SemanticToken>>();

    assert_eq!(
        result,
        &[
            SemanticToken {
                line: 3,
                start: 5,
                length: 3,
                token_type: TokenType(0),
                token_modifiers: 3
            },
            SemanticToken {
                line: 3,
                start: 10,
                length: 4,
                token_type: TokenType(1),
                token_modifiers: 0
            },
            SemanticToken {
                line: 6,
                start: 2,
                length: 7,
                token_type: TokenType(2),
                token_modifiers: 0
            }
        ]
    );
}

#[test]
fn applies_out_of_bounds_delta_edit_without_panic() {
    let mut tokens = ServerSemanticTokens::from_full(vec![2, 5, 3, 0, 3, 0, 5, 4, 1, 0], None);

    tokens.apply(&[SemanticTokensEdit {
        start: 100,
        delete_count: 5,
        data: vec![1, 2, 3, 4, 5],
    }]);
    assert_eq!(
        tokens.data,
        vec![2, 5, 3, 0, 3, 0, 5, 4, 1, 0, 1, 2, 3, 4, 5]
    );

    let mut tokens = ServerSemanticTokens::from_full(vec![2, 5, 3, 0, 3], None);
    tokens.apply(&[SemanticTokensEdit {
        start: 3,
        delete_count: 100,
        data: vec![9, 9],
    }]);
    assert_eq!(tokens.data, vec![2, 5, 3, 9, 9]);

    let mut tokens = ServerSemanticTokens::from_full(Vec::new(), None);
    tokens.apply(&[SemanticTokensEdit {
        start: 0,
        delete_count: 5,
        data: vec![1, 2, 3, 4, 5],
    }]);
    assert_eq!(tokens.data, vec![1, 2, 3, 4, 5]);
}

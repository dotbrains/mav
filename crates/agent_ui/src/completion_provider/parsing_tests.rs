use super::*;
use gpui::TestAppContext;

#[test]
fn test_prompt_completion_parse() {
    let supported_modes = vec![PromptContextType::File, PromptContextType::Symbol];

    assert_eq!(
        PromptCompletion::try_parse("/", 0, &supported_modes),
        Some(PromptCompletion::SlashCommand(SlashCommandCompletion {
            source_range: 0..1,
            command: None,
            argument: None,
        }))
    );

    assert_eq!(
        PromptCompletion::try_parse("@", 0, &supported_modes),
        Some(PromptCompletion::Mention(MentionCompletion {
            source_range: 0..1,
            mode: None,
            argument: None,
        }))
    );

    assert_eq!(
        PromptCompletion::try_parse("/test @file", 0, &supported_modes),
        Some(PromptCompletion::Mention(MentionCompletion {
            source_range: 6..11,
            mode: Some(PromptContextType::File),
            argument: None,
        }))
    );
}

#[test]
fn test_slash_command_completion_parse() {
    assert_eq!(
        SlashCommandCompletion::try_parse("/", 0),
        Some(SlashCommandCompletion {
            source_range: 0..1,
            command: None,
            argument: None,
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("/help", 0),
        Some(SlashCommandCompletion {
            source_range: 0..5,
            command: Some("help".to_string()),
            argument: None,
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("/help ", 0),
        Some(SlashCommandCompletion {
            source_range: 0..5,
            command: Some("help".to_string()),
            argument: None,
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("/help arg1", 0),
        Some(SlashCommandCompletion {
            source_range: 0..10,
            command: Some("help".to_string()),
            argument: Some("arg1".to_string()),
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("/help arg1 arg2", 0),
        Some(SlashCommandCompletion {
            source_range: 0..15,
            command: Some("help".to_string()),
            argument: Some("arg1 arg2".to_string()),
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("/拿不到命令 拿不到命令 ", 0),
        Some(SlashCommandCompletion {
            source_range: 0..30,
            command: Some("拿不到命令".to_string()),
            argument: Some("拿不到命令".to_string()),
        })
    );

    assert_eq!(SlashCommandCompletion::try_parse("Lorem Ipsum", 0), None);

    assert_eq!(
        SlashCommandCompletion::try_parse("Lorem /", 0),
        Some(SlashCommandCompletion {
            source_range: 6..7,
            command: None,
            argument: None,
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("Lorem /help", 0),
        Some(SlashCommandCompletion {
            source_range: 6..11,
            command: Some("help".to_string()),
            argument: None,
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("Lorem /help /test", 0),
        Some(SlashCommandCompletion {
            source_range: 12..17,
            command: Some("test".to_string()),
            argument: None,
        })
    );

    assert_eq!(
        SlashCommandCompletion::try_parse("/help", 10),
        Some(SlashCommandCompletion {
            source_range: 10..15,
            command: Some("help".to_string()),
            argument: None,
        })
    );

    assert_eq!(SlashCommandCompletion::try_parse("Lorem/", 0), None);

    assert_eq!(SlashCommandCompletion::try_parse("/ ", 0), None);
}

#[test]
fn test_section_headers_visible_until_argument() {
    // Section headers stay visible while the user narrows the command name
    // (`/`, `/comp`, `/compact `) and only disappear once they start typing
    // the command's argument, where category grouping no longer applies.
    let show_section_headers = |input: &str| {
        SlashCommandCompletion::try_parse(input, 0)
            .unwrap()
            .argument
            .is_none()
    };

    assert!(show_section_headers("/"));
    assert!(show_section_headers("/comp"));
    assert!(show_section_headers("/compact"));
    assert!(show_section_headers("/compact "));
    assert!(!show_section_headers("/compact now"));
}

#[test]
fn test_group_by_relevance_floats_best_group_and_keeps_groups_contiguous() {
    // Items arrive in fuzzy-score order (best first). The group containing
    // the best match floats to the top, groups stay contiguous, and the
    // within-group order is preserved.
    let mut items = [
        ("compact", 1u32),  // best match, group 1
        ("skill-a", 0u32),  // group 0
        ("deploy", 2u32),   // group 2
        ("skill-b", 0u32),  // group 0 (after skill-a in score order)
        ("native-b", 1u32), // group 1 (after compact)
    ];
    group_by_relevance(&mut items, |(_, key)| *key);
    let order: Vec<&str> = items.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        order,
        vec!["compact", "native-b", "skill-a", "skill-b", "deploy"]
    );

    // When the best match is a skill, the skill group leads instead.
    let mut items = [("skill-a", 0u32), ("compact", 1u32)];
    group_by_relevance(&mut items, |(_, key)| *key);
    let order: Vec<&str> = items.iter().map(|(name, _)| *name).collect();
    assert_eq!(order, vec!["skill-a", "compact"]);
}

#[test]
fn test_mention_completion_parse() {
    let supported_modes = vec![PromptContextType::File, PromptContextType::Symbol];
    let supported_modes_with_diagnostics = vec![
        PromptContextType::File,
        PromptContextType::Symbol,
        PromptContextType::Diagnostics,
    ];

    assert_eq!(
        MentionCompletion::try_parse("Lorem Ipsum", 0, &supported_modes),
        None
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..7,
            mode: None,
            argument: None,
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @file", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..11,
            mode: Some(PromptContextType::File),
            argument: None,
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @file ", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..12,
            mode: Some(PromptContextType::File),
            argument: None,
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @file main.rs", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..19,
            mode: Some(PromptContextType::File),
            argument: Some("main.rs".to_string()),
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @file main.rs ", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..19,
            mode: Some(PromptContextType::File),
            argument: Some("main.rs".to_string()),
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @file main.rs Ipsum", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..19,
            mode: Some(PromptContextType::File),
            argument: Some("main.rs".to_string()),
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @main", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..11,
            mode: None,
            argument: Some("main".to_string()),
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @main ", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..12,
            mode: None,
            argument: Some("main".to_string()),
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @main m", 0, &supported_modes),
        None
    );

    assert_eq!(
        MentionCompletion::try_parse("test@", 0, &supported_modes),
        None
    );

    // Allowed non-file mentions

    assert_eq!(
        MentionCompletion::try_parse("Lorem @symbol main", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 6..18,
            mode: Some(PromptContextType::Symbol),
            argument: Some("main".to_string()),
        })
    );

    assert_eq!(
        MentionCompletion::try_parse(
            "Lorem @symbol agent_ui::completion_provider",
            0,
            &supported_modes
        ),
        Some(MentionCompletion {
            source_range: 6..43,
            mode: Some(PromptContextType::Symbol),
            argument: Some("agent_ui::completion_provider".to_string()),
        })
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @diagnostics", 0, &supported_modes_with_diagnostics),
        Some(MentionCompletion {
            source_range: 6..18,
            mode: Some(PromptContextType::Diagnostics),
            argument: None,
        })
    );

    // Disallowed non-file mentions
    assert_eq!(
        MentionCompletion::try_parse("Lorem @symbol main", 0, &[PromptContextType::File]),
        None
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem@symbol", 0, &supported_modes),
        None,
        "Should not parse mention inside word"
    );

    assert_eq!(
        MentionCompletion::try_parse("Lorem @ file", 0, &supported_modes),
        None,
        "Should not parse with a space after @"
    );

    assert_eq!(
        MentionCompletion::try_parse("@ file", 0, &supported_modes),
        None,
        "Should not parse with a space after @ at the start of the line"
    );

    assert_eq!(
        MentionCompletion::try_parse(
            "@fetch https://www.npmjs.com/package/@matterport/sdk",
            0,
            &[PromptContextType::Fetch]
        ),
        Some(MentionCompletion {
            source_range: 0..52,
            mode: Some(PromptContextType::Fetch),
            argument: Some("https://www.npmjs.com/package/@matterport/sdk".to_string()),
        }),
        "Should handle URLs with @ in the path"
    );

    assert_eq!(
        MentionCompletion::try_parse(
            "@fetch https://example.com/@org/@repo/file",
            0,
            &[PromptContextType::Fetch]
        ),
        Some(MentionCompletion {
            source_range: 0..42,
            mode: Some(PromptContextType::Fetch),
            argument: Some("https://example.com/@org/@repo/file".to_string()),
        }),
        "Should handle URLs with multiple @ characters"
    );

    assert_eq!(
        MentionCompletion::try_parse(
            "@fetch https://example.com/@",
            0,
            &[PromptContextType::Fetch]
        ),
        Some(MentionCompletion {
            source_range: 0..28,
            mode: Some(PromptContextType::Fetch),
            argument: Some("https://example.com/@".to_string()),
        }),
        "Should parse URL ending with @ (even if URL is incomplete)"
    );

    // Bracketed mentions: opening brackets count as a boundary before '@' so
    // typing `(@`, `[@`, or `{@` still opens the completion menu.

    assert_eq!(
        MentionCompletion::try_parse("(@", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 1..2,
            mode: None,
            argument: None,
        }),
        "Should parse mention immediately after '('"
    );

    assert_eq!(
        MentionCompletion::try_parse("[@", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 1..2,
            mode: None,
            argument: None,
        }),
        "Should parse mention immediately after '['"
    );

    assert_eq!(
        MentionCompletion::try_parse("{@", 0, &supported_modes),
        Some(MentionCompletion {
            source_range: 1..2,
            mode: None,
            argument: None,
        }),
        "Should parse mention immediately after '{{'"
    );
}

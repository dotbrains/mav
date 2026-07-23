use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_settings_preserves_display_names_for_similar_models() {
        // Regression test for https://github.com/mav-industries/mav/issues/43646
        // When multiple models share the same base name (e.g., qwen2.5-coder:1.5b and qwen2.5-coder:3b),
        // each model should get its own display_name from settings, not a random one.

        let mut models: HashMap<String, ollama::Model> = HashMap::default();
        models.insert(
            "qwen2.5-coder:1.5b".to_string(),
            ollama::Model {
                name: "qwen2.5-coder:1.5b".to_string(),
                display_name: None,
                max_tokens: 4096,
                keep_alive: None,
                supports_tools: None,
                supports_vision: None,
                supports_thinking: None,
                disabled: None,
            },
        );
        models.insert(
            "qwen2.5-coder:3b".to_string(),
            ollama::Model {
                name: "qwen2.5-coder:3b".to_string(),
                display_name: None,
                max_tokens: 4096,
                keep_alive: None,
                supports_tools: None,
                supports_vision: None,
                supports_thinking: None,
                disabled: None,
            },
        );

        let available_models = vec![
            AvailableModel {
                name: "qwen2.5-coder:1.5b".to_string(),
                display_name: Some("QWEN2.5 Coder 1.5B".to_string()),
                max_tokens: 5000,
                keep_alive: None,
                supports_tools: Some(true),
                supports_images: None,
                supports_thinking: None,
            },
            AvailableModel {
                name: "qwen2.5-coder:3b".to_string(),
                display_name: Some("QWEN2.5 Coder 3B".to_string()),
                max_tokens: 6000,
                keep_alive: None,
                supports_tools: Some(true),
                supports_images: None,
                supports_thinking: None,
            },
        ];

        merge_settings_into_models(&mut models, &available_models, None);

        let model_1_5b = models
            .get("qwen2.5-coder:1.5b")
            .expect("1.5b model missing");
        let model_3b = models.get("qwen2.5-coder:3b").expect("3b model missing");

        assert_eq!(
            model_1_5b.display_name,
            Some("QWEN2.5 Coder 1.5B".to_string()),
            "1.5b model should have its own display_name"
        );
        assert_eq!(model_1_5b.max_tokens, 5000);

        assert_eq!(
            model_3b.display_name,
            Some("QWEN2.5 Coder 3B".to_string()),
            "3b model should have its own display_name"
        );
        assert_eq!(model_3b.max_tokens, 6000);
    }
}

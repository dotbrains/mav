use super::*;
use crate::fake_provider::FakeLanguageModelProvider;

#[test]
fn selected_model_allows_slashes_in_model_id() {
    let selected = SelectedModel::from_str("custom-provider/organization/model-name")
        .expect("model identifier should parse");

    assert_eq!(
        selected.provider,
        LanguageModelProviderId("custom-provider".into())
    );
    assert_eq!(
        selected.model,
        LanguageModelId("organization/model-name".into())
    );
}

#[test]
fn selected_model_rejects_missing_separator_or_empty_parts() {
    assert!(SelectedModel::from_str("custom-provider").is_err());
    assert!(SelectedModel::from_str("/organization/model-name").is_err());
    assert!(SelectedModel::from_str("custom-provider/").is_err());
}

#[gpui::test]
fn test_register_providers(cx: &mut App) {
    let registry = cx.new(|_| LanguageModelRegistry::default());

    let provider = Arc::new(FakeLanguageModelProvider::default());
    registry.update(cx, |registry, cx| {
        registry.register_provider(provider.clone(), cx);
    });

    let providers = registry.read(cx).providers();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].id(), provider.id());

    registry.update(cx, |registry, cx| {
        registry.unregister_provider(provider.id(), cx);
    });

    let providers = registry.read(cx).providers();
    assert!(providers.is_empty());
}

#[gpui::test]
fn test_provider_hiding_on_extension_install(cx: &mut App) {
    let registry = cx.new(|_| LanguageModelRegistry::default());

    let provider = Arc::new(FakeLanguageModelProvider::default());
    let provider_id = provider.id();

    registry.update(cx, |registry, cx| {
        registry.register_provider(provider.clone(), cx);

        registry.set_builtin_provider_hiding_fn(Box::new(|id| {
            if id == "fake" {
                Some("fake-extension")
            } else {
                None
            }
        }));
    });

    let visible = registry.read(cx).visible_providers();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id(), provider_id);

    registry.update(cx, |registry, cx| {
        registry.extension_installed("fake-extension".into(), cx);
    });

    let visible = registry.read(cx).visible_providers();
    assert!(visible.is_empty());

    let all = registry.read(cx).providers();
    assert_eq!(all.len(), 1);
}

#[gpui::test]
fn test_provider_unhiding_on_extension_uninstall(cx: &mut App) {
    let registry = cx.new(|_| LanguageModelRegistry::default());

    let provider = Arc::new(FakeLanguageModelProvider::default());
    let provider_id = provider.id();

    registry.update(cx, |registry, cx| {
        registry.register_provider(provider.clone(), cx);

        registry.set_builtin_provider_hiding_fn(Box::new(|id| {
            if id == "fake" {
                Some("fake-extension")
            } else {
                None
            }
        }));

        registry.extension_installed("fake-extension".into(), cx);
    });

    let visible = registry.read(cx).visible_providers();
    assert!(visible.is_empty());

    registry.update(cx, |registry, cx| {
        registry.extension_uninstalled("fake-extension", cx);
    });

    let visible = registry.read(cx).visible_providers();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id(), provider_id);
}

#[gpui::test]
fn test_should_hide_provider(cx: &mut App) {
    let registry = cx.new(|_| LanguageModelRegistry::default());

    registry.update(cx, |registry, cx| {
        registry.set_builtin_provider_hiding_fn(Box::new(|id| {
            if id == "anthropic" {
                Some("anthropic")
            } else if id == "openai" {
                Some("openai")
            } else {
                None
            }
        }));

        registry.extension_installed("anthropic".into(), cx);
    });

    let registry_read = registry.read(cx);

    assert!(registry_read.should_hide_provider(&LanguageModelProviderId("anthropic".into())));

    assert!(!registry_read.should_hide_provider(&LanguageModelProviderId("openai".into())));

    assert!(!registry_read.should_hide_provider(&LanguageModelProviderId("unknown".into())));
}

#[gpui::test]
async fn test_configure_environment_fallback_model(cx: &mut gpui::TestAppContext) {
    let registry = cx.new(|_| LanguageModelRegistry::default());

    let provider = Arc::new(FakeLanguageModelProvider::default());
    registry.update(cx, |registry, cx| {
        registry.register_provider(provider.clone(), cx);
    });

    cx.update(|cx| provider.authenticate(cx)).await.unwrap();

    registry.update(cx, |registry, cx| {
        let provider = registry.provider(&provider.id()).unwrap();
        let model = provider.default_model(cx).unwrap();

        registry.set_environment_fallback_model(
            Some(ConfiguredModel {
                provider: provider.clone(),
                model: model.clone(),
            }),
            cx,
        );

        let default_model = registry.default_model().unwrap();
        assert_eq!(default_model.model.id(), model.id());
        assert_eq!(default_model.provider.id(), provider.id());
    });
}

#[gpui::test]
fn test_sync_installed_llm_extensions(cx: &mut App) {
    let registry = cx.new(|_| LanguageModelRegistry::default());

    let provider = Arc::new(FakeLanguageModelProvider::default());

    registry.update(cx, |registry, cx| {
        registry.register_provider(provider.clone(), cx);

        registry.set_builtin_provider_hiding_fn(Box::new(|id| {
            if id == "fake" {
                Some("fake-extension")
            } else {
                None
            }
        }));
    });

    let mut extension_ids = HashSet::default();
    extension_ids.insert(Arc::from("fake-extension"));

    registry.update(cx, |registry, cx| {
        registry.sync_installed_llm_extensions(extension_ids, cx);
    });

    assert!(registry.read(cx).visible_providers().is_empty());

    registry.update(cx, |registry, cx| {
        registry.sync_installed_llm_extensions(HashSet::default(), cx);
    });

    assert_eq!(registry.read(cx).visible_providers().len(), 1);
}

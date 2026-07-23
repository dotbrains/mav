
use super::*;
use anyhow::Result;
use clock::FakeSystemClock;
use feature_flags::FeatureFlagAppExt as _;
use gpui::{AppContext as _, AsyncApp, BorrowAppContext as _};
use http_client::FakeHttpClient;
use language_model::IconOrSvg;
use release_channel::AppVersion;
use std::future::Future;
use std::pin::Pin;
use ui::IconName;

struct FakeCredentialsProvider;

impl CredentialsProvider for FakeCredentialsProvider {
    fn read_credentials<'a>(
        &'a self,
        _url: &'a str,
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<Option<(String, Vec<u8>)>>> + 'a>> {
        Box::pin(async { Ok(None) })
    }

    fn write_credentials<'a>(
        &'a self,
        _url: &'a str,
        _username: &'a str,
        _password: &'a [u8],
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn delete_credentials<'a>(
        &'a self,
        _url: &'a str,
        _cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

fn init_test(cx: &mut App) -> (Arc<Client>, Arc<dyn CredentialsProvider>) {
    let settings_store = SettingsStore::test(cx);
    cx.set_global(settings_store);
    cx.set_global(db::AppDatabase::test_new());
    let app_version = AppVersion::global(cx);
    release_channel::init_test(app_version, release_channel::ReleaseChannel::Dev, cx);
    gpui_tokio::init(cx);
    cx.update_flags(false, Vec::new());

    let client = Client::new(
        Arc::new(FakeSystemClock::new()),
        FakeHttpClient::with_404_response(),
        cx,
    );
    (client, Arc::new(FakeCredentialsProvider))
}

fn update_compatible_provider_settings(
    openai: &[&str],
    anthropic: &[&str],
    cx: &mut App,
) -> CompatibleProviders {
    fn section(ids: &[&str]) -> serde_json::Value {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    serde_json::json!({
                        "api_url": "https://example.com",
                        "available_models": [],
                    }),
                )
            })
            .collect::<serde_json::Map<String, serde_json::Value>>()
            .into()
    }

    let content = serde_json::json!({
        "language_models": {
            "openai_compatible": section(openai),
            "anthropic_compatible": section(anthropic),
        }
    })
    .to_string();
    cx.update_global::<SettingsStore, _>(|store, cx| {
        store
            .set_user_settings(&content, cx)
            .expect("failed to parse test settings");
    });
    CompatibleProviders::from_settings(cx)
}

fn provider_icons(registry: &LanguageModelRegistry, id: &str) -> Vec<IconOrSvg> {
    registry
        .providers()
        .into_iter()
        .filter(|provider| provider.id().0.as_ref() == id)
        .map(|provider| provider.icon())
        .collect()
}

#[gpui::test]
fn test_compatible_provider_id_collision_resolves_when_one_entry_is_removed(cx: &mut App) {
    let (client, credentials_provider) = init_test(cx);
    let registry = cx.new(|_| LanguageModelRegistry::default());

    // The same provider name is configured in both `openai_compatible`
    // and `anthropic_compatible` settings sections; the OpenAI-compatible
    // entry wins the collision.
    let both = update_compatible_provider_settings(&["acme"], &["acme"], cx);
    registry.update(cx, |registry, cx| {
        register_compatible_providers(
            registry,
            &CompatibleProviders::default(),
            &both,
            &client,
            &credentials_provider,
            cx,
        );
    });
    assert_eq!(
        registry.read_with(cx, |registry, _| provider_icons(registry, "acme")),
        vec![IconOrSvg::Icon(IconName::AiOpenAiCompat)],
        "the OpenAI-compatible provider should win the name collision"
    );

    // The user removes the `anthropic_compatible` entry; the remaining
    // `openai_compatible` entry must stay registered.
    let openai_only = update_compatible_provider_settings(&["acme"], &[], cx);
    registry.update(cx, |registry, cx| {
        register_compatible_providers(
            registry,
            &both,
            &openai_only,
            &client,
            &credentials_provider,
            cx,
        );
    });
    assert_eq!(
        registry.read_with(cx, |registry, _| provider_icons(registry, "acme")),
        vec![IconOrSvg::Icon(IconName::AiOpenAiCompat)],
        "the provider registered for `acme` should be the OpenAI-compatible one"
    );
}

#[gpui::test]
fn test_compatible_provider_changes_kind_and_unregisters(cx: &mut App) {
    let (client, credentials_provider) = init_test(cx);
    let registry = cx.new(|_| LanguageModelRegistry::default());

    let both = update_compatible_provider_settings(&["acme"], &["acme"], cx);
    registry.update(cx, |registry, cx| {
        register_compatible_providers(
            registry,
            &CompatibleProviders::default(),
            &both,
            &client,
            &credentials_provider,
            cx,
        );
    });

    // Removing the `openai_compatible` entry hands the name over to the
    // remaining `anthropic_compatible` entry.
    let anthropic_only = update_compatible_provider_settings(&[], &["acme"], cx);
    registry.update(cx, |registry, cx| {
        register_compatible_providers(
            registry,
            &both,
            &anthropic_only,
            &client,
            &credentials_provider,
            cx,
        );
    });
    assert_eq!(
        registry.read_with(cx, |registry, _| provider_icons(registry, "acme")),
        vec![IconOrSvg::Icon(IconName::AiAnthropicCompat)],
        "after removing the openai_compatible entry, the anthropic_compatible provider should be registered"
    );

    // Removing the last entry unregisters the provider entirely.
    let none = update_compatible_provider_settings(&[], &[], cx);
    registry.update(cx, |registry, cx| {
        register_compatible_providers(
            registry,
            &anthropic_only,
            &none,
            &client,
            &credentials_provider,
            cx,
        );
    });
    assert_eq!(
        registry.read_with(cx, |registry, _| provider_icons(registry, "acme")),
        Vec::new(),
        "removing all entries should unregister the provider"
    );
}

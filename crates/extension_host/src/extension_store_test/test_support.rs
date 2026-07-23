use super::*;

async fn await_or_timeout<T>(
    executor: &BackgroundExecutor,
    what: &'static str,
    seconds: u64,
    future: impl std::future::Future<Output = T>,
) -> T {
    let timeout = executor.timer(std::time::Duration::from_secs(seconds));

    futures::select! {
        output = future.fuse() => output,
        _ = futures::FutureExt::fuse(timeout) => panic!(
        "[test_extension_store_with_test_extension] timed out after {seconds}s while {what}"
    )
    }
}

struct FakeLanguageServerVersion {
    version: String,
    binary_contents: String,
    http_request_count: usize,
}

pub(super) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let store = SettingsStore::test(cx);
        cx.set_global(store);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        extension::init(cx);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        gpui_tokio::init(cx);
    });
}

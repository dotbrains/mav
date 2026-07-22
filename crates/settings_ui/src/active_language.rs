use gpui::SharedString;
use std::sync::{LazyLock, RwLock};

/// The current sub page path that is selected.
/// If this is empty the selected page is rendered,
/// otherwise the last sub page gets rendered.
///
/// Global so that `pick` and `write` callbacks can access it
/// and use it to dynamically render sub pages (e.g. for language settings)
static ACTIVE_LANGUAGE: LazyLock<RwLock<Option<SharedString>>> =
    LazyLock::new(|| RwLock::new(Option::None));

pub(crate) fn active_language() -> Option<SharedString> {
    ACTIVE_LANGUAGE
        .read()
        .ok()
        .and_then(|language| language.clone())
}

pub(crate) fn active_language_mut()
-> Option<std::sync::RwLockWriteGuard<'static, Option<SharedString>>> {
    ACTIVE_LANGUAGE.write().ok()
}

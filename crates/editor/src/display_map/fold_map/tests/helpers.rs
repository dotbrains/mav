use super::*;
use settings::SettingsStore;

fn init_test(cx: &mut gpui::App) {
    let store = SettingsStore::test(cx);
    cx.set_global(store);
}

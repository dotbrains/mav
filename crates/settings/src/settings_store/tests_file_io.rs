use super::tests_common::*;
use super::*;

#[gpui::test]
async fn test_update_settings_file_updates_store_before_watcher(cx: &mut gpui::TestAppContext) {
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.create_dir(paths::settings_file().parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        paths::settings_file(),
        r#"{ "tabs": { "close_position": "right" } }"#.as_bytes().to_vec(),
    )
    .await;
    fs.pause_events();
    cx.run_until_parked();

    let success = SettingsParseResult {
        parse_status: ParseStatus::Success,
        migration_status: MigrationStatus::NotNeeded,
    };
    let parse_results = Rc::new(RefCell::new(Vec::new()));

    cx.update(|cx| {
        let mut store = SettingsStore::new(cx, &default_settings());
        store.register_setting::<ItemSettings>();
        store.watch_settings_files(fs.clone(), cx, {
            let parse_results = parse_results.clone();
            move |_, result, _| {
                parse_results.borrow_mut().push(result);
            }
        });
        cx.set_global(store);
    });

    // Calling watch_settings_files loads user and global settings.
    assert_eq!(
        parse_results.borrow().as_slice(),
        &[success.clone(), success.clone()]
    );
    cx.update(|cx| {
        assert_eq!(
            cx.global::<SettingsStore>()
                .get::<ItemSettings>(None)
                .close_position,
            ClosePosition::Right
        );
    });

    // Updating the settings file returns a channel that resolves once the settings are loaded.
    let rx = cx.update(|cx| {
        cx.global::<SettingsStore>()
            .update_settings_file_with_completion(fs.clone(), move |settings, _| {
                settings.tabs.get_or_insert_default().close_position = Some(ClosePosition::Left);
            })
    });
    assert!(rx.await.unwrap().is_ok());
    assert_eq!(
        parse_results.borrow().as_slice(),
        &[success.clone(), success.clone()]
    );
    cx.update(|cx| {
        assert_eq!(
            cx.global::<SettingsStore>()
                .get::<ItemSettings>(None)
                .close_position,
            ClosePosition::Left
        );
    });

    // When the FS event occurs, the settings are recognized as unchanged.
    fs.flush_events(100);
    cx.run_until_parked();
    assert_eq!(
        parse_results.borrow().as_slice(),
        &[
            success.clone(),
            success.clone(),
            SettingsParseResult {
                parse_status: ParseStatus::Unchanged,
                migration_status: MigrationStatus::NotNeeded
            }
        ]
    );
}

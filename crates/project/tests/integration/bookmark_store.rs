pub(crate) use std::{path::Path, sync::Arc};

pub(crate) use collections::BTreeMap;
pub(crate) use gpui::{Entity, TestAppContext};
pub(crate) use language::Buffer;
pub(crate) use project::{Project, bookmark_store::SerializedBookmark};
pub(crate) use serde_json::json;
pub(crate) use util::path;

pub(crate) use fs::Fs as _;

pub(crate) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
}

pub(crate) fn project_path(path: &str) -> Arc<Path> {
    Arc::from(Path::new(path))
}

pub(crate) fn serialized_bookmark(row: u32) -> SerializedBookmark {
    SerializedBookmark {
        row,
        label: String::new(),
    }
}

pub(crate) async fn open_buffer(
    project: &Entity<Project>,
    path: &str,
    cx: &mut TestAppContext,
) -> Entity<Buffer> {
    project
        .update(cx, |project, cx| {
            project.open_local_buffer(Path::new(path), cx)
        })
        .await
        .unwrap()
}

pub(crate) fn add_bookmarks(
    project: &Entity<Project>,
    buffer: &Entity<Buffer>,
    rows: &[u32],
    cx: &mut TestAppContext,
) {
    let buffer = buffer.clone();
    project.update(cx, |project, cx| {
        let bookmark_store = project.bookmark_store();
        let snapshot = buffer.read(cx).snapshot();
        for &row in rows {
            let anchor = snapshot.anchor_after(text::Point::new(row, 0));
            bookmark_store.update(cx, |store, cx| {
                store.toggle_bookmark(buffer.clone(), anchor, String::new(), cx);
            });
        }
    });
}

pub(crate) fn add_labeled_bookmark(
    project: &Entity<Project>,
    buffer: &Entity<Buffer>,
    row: u32,
    label: &str,
    cx: &mut TestAppContext,
) {
    let buffer = buffer.clone();
    project.update(cx, |project, cx| {
        let bookmark_store = project.bookmark_store();
        let snapshot = buffer.read(cx).snapshot();
        let anchor = snapshot.anchor_after(text::Point::new(row, 0));
        bookmark_store.update(cx, |store, cx| {
            store.toggle_bookmark(buffer.clone(), anchor, label.to_string(), cx);
        });
    });
}

pub(crate) fn get_all_bookmarks(
    project: &Entity<Project>,
    cx: &mut TestAppContext,
) -> BTreeMap<Arc<Path>, Vec<SerializedBookmark>> {
    project.read_with(cx, |project, cx| {
        project
            .bookmark_store()
            .read(cx)
            .all_serialized_bookmarks(cx)
    })
}

pub(crate) fn build_serialized(
    entries: &[(&str, &[u32])],
) -> BTreeMap<Arc<Path>, Vec<SerializedBookmark>> {
    let mut map = BTreeMap::new();
    for &(path_str, rows) in entries {
        let path = project_path(path_str);
        map.insert(
            path.clone(),
            rows.iter().map(|&row| serialized_bookmark(row)).collect(),
        );
    }
    map
}

pub(crate) async fn restore_bookmarks(
    project: &Entity<Project>,
    serialized: BTreeMap<Arc<Path>, Vec<SerializedBookmark>>,
    cx: &mut TestAppContext,
) {
    project
        .update(cx, |project, cx| {
            project.bookmark_store().update(cx, |store, cx| {
                store.load_serialized_bookmarks(serialized, cx)
            })
        })
        .await
        .expect("with_serialized_bookmarks should succeed");
}

pub(crate) fn clear_bookmarks(project: &Entity<Project>, cx: &mut TestAppContext) {
    project.update(cx, |project, cx| {
        project.bookmark_store().update(cx, |store, cx| {
            store.clear_bookmarks(cx);
        });
    });
}

pub(crate) fn assert_bookmark_rows(
    bookmarks: &BTreeMap<Arc<Path>, Vec<SerializedBookmark>>,
    path: &str,
    expected_rows: &[u32],
) {
    let path = project_path(path);
    let file_bookmarks = bookmarks
        .get(&path)
        .unwrap_or_else(|| panic!("Expected bookmarks for {}", path.display()));
    let rows: Vec<u32> = file_bookmarks.iter().map(|b| b.row).collect();
    assert_eq!(rows, expected_rows, "Bookmark rows for {}", path.display());
}

pub(crate) fn assert_bookmark_labels(
    bookmarks: &BTreeMap<Arc<Path>, Vec<SerializedBookmark>>,
    path: &str,
    expected: &[(u32, &str)],
) {
    let path = project_path(path);
    let file_bookmarks = bookmarks
        .get(&path)
        .unwrap_or_else(|| panic!("Expected bookmarks for {}", path.display()));
    let actual: Vec<_> = file_bookmarks
        .iter()
        .map(|bookmark| (bookmark.row, bookmark.label.as_str()))
        .collect();
    assert_eq!(actual, expected, "Bookmark labels for {}", path.display());
}

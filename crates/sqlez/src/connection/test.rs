use anyhow::Result;
use indoc::indoc;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::connection::Connection;

static NEXT_NAMED_MEMORY_DB_ID: AtomicUsize = AtomicUsize::new(0);

fn unique_named_memory_db(prefix: &str) -> String {
    format!(
        "{prefix}_{}_{}",
        std::process::id(),
        NEXT_NAMED_MEMORY_DB_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn literal_named_memory_paths(name: &str) -> [String; 3] {
    let main = format!("file:{name}?mode=memory&cache=shared");
    [main.clone(), format!("{main}-wal"), format!("{main}-shm")]
}

struct NamedMemoryPathGuard {
    paths: [String; 3],
}

impl NamedMemoryPathGuard {
    fn new(name: &str) -> Self {
        let paths = literal_named_memory_paths(name);
        for path in &paths {
            let _ = fs::remove_file(path);
        }
        Self { paths }
    }
}

impl Drop for NamedMemoryPathGuard {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = fs::remove_file(path);
        }
    }
}

#[test]
fn string_round_trips() -> Result<()> {
    let connection = Connection::open_memory(Some("string_round_trips"));
    connection
        .exec(indoc! {"
        CREATE TABLE text (
            text TEXT
        );"})
        .unwrap()()
    .unwrap();

    let text = "Some test text";

    connection
        .exec_bound("INSERT INTO text (text) VALUES (?);")
        .unwrap()(text)
    .unwrap();

    assert_eq!(
        connection.select_row("SELECT text FROM text;").unwrap()().unwrap(),
        Some(text.to_string())
    );

    Ok(())
}

#[test]
fn tuple_round_trips() {
    let connection = Connection::open_memory(Some("tuple_round_trips"));
    connection
        .exec(indoc! {"
            CREATE TABLE test (
                text TEXT,
                integer INTEGER,
                blob BLOB
            );"})
        .unwrap()()
    .unwrap();

    let tuple1 = ("test".to_string(), 64, vec![0, 1, 2, 4, 8, 16, 32, 64]);
    let tuple2 = ("test2".to_string(), 32, vec![64, 32, 16, 8, 4, 2, 1, 0]);

    let mut insert = connection
        .exec_bound::<(String, usize, Vec<u8>)>(
            "INSERT INTO test (text, integer, blob) VALUES (?, ?, ?)",
        )
        .unwrap();

    insert(tuple1.clone()).unwrap();
    insert(tuple2.clone()).unwrap();

    assert_eq!(
        connection
            .select::<(String, usize, Vec<u8>)>("SELECT * FROM test")
            .unwrap()()
        .unwrap(),
        vec![tuple1, tuple2]
    );
}

#[test]
fn bool_round_trips() {
    let connection = Connection::open_memory(Some("bool_round_trips"));
    connection
        .exec(indoc! {"
            CREATE TABLE bools (
                t INTEGER,
                f INTEGER
            );"})
        .unwrap()()
    .unwrap();

    connection
        .exec_bound("INSERT INTO bools(t, f) VALUES (?, ?)")
        .unwrap()((true, false))
    .unwrap();

    assert_eq!(
        connection
            .select_row::<(bool, bool)>("SELECT * FROM bools;")
            .unwrap()()
        .unwrap(),
        Some((true, false))
    );
}

#[test]
fn backup_works() {
    let connection1 = Connection::open_memory(Some("backup_works"));
    connection1
        .exec(indoc! {"
            CREATE TABLE blobs (
                data BLOB
            );"})
        .unwrap()()
    .unwrap();
    let blob = vec![0, 1, 2, 4, 8, 16, 32, 64];
    connection1
        .exec_bound::<Vec<u8>>("INSERT INTO blobs (data) VALUES (?);")
        .unwrap()(blob.clone())
    .unwrap();

    let connection2 = Connection::open_memory(Some("backup_works_other"));
    connection1.backup_main(&connection2).unwrap();

    let read_blobs = connection1
        .select::<Vec<u8>>("SELECT * FROM blobs;")
        .unwrap()()
    .unwrap();
    assert_eq!(read_blobs, vec![blob]);
}

#[test]
fn named_memory_connections_do_not_create_literal_backing_files() {
    let name = unique_named_memory_db("named_memory_connections_do_not_create_backing_files");
    let guard = NamedMemoryPathGuard::new(&name);

    let connection1 = Connection::open_memory(Some(&name));
    connection1
        .exec(indoc! {"
            CREATE TABLE shared (
                value INTEGER
            )"})
        .unwrap()()
    .unwrap();
    connection1
        .exec("INSERT INTO shared (value) VALUES (7)")
        .unwrap()()
    .unwrap();

    let connection2 = Connection::open_memory(Some(&name));
    assert_eq!(
        connection2
            .select_row::<i64>("SELECT value FROM shared")
            .unwrap()()
        .unwrap(),
        Some(7)
    );

    for path in &guard.paths {
        assert!(
            fs::metadata(path).is_err(),
            "named in-memory database unexpectedly created backing file {path}"
        );
    }
}

#[test]
fn multi_step_statement_works() {
    let connection = Connection::open_memory(Some("multi_step_statement_works"));

    connection
        .exec(indoc! {"
            CREATE TABLE test (
                col INTEGER
            )"})
        .unwrap()()
    .unwrap();

    connection
        .exec(indoc! {"
        INSERT INTO test(col) VALUES (2)"})
        .unwrap()()
    .unwrap();

    assert_eq!(
        connection
            .select_row::<usize>("SELECT * FROM test")
            .unwrap()()
        .unwrap(),
        Some(2)
    );
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
#[test]
fn test_sql_has_syntax_errors() {
    let connection = Connection::open_memory(Some("test_sql_has_syntax_errors"));
    let first_stmt = "CREATE TABLE kv_store(key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT ;";
    let second_stmt = "SELECT FROM";

    let second_offset = connection.sql_has_syntax_error(second_stmt).unwrap().1;

    let res = connection
        .sql_has_syntax_error(&format!("{}\n{}", first_stmt, second_stmt))
        .map(|(_, offset)| offset);

    assert_eq!(res, Some(first_stmt.len() + second_offset + 1));
}

#[test]
fn test_alter_table_syntax() {
    let connection = Connection::open_memory(Some("test_alter_table_syntax"));

    assert!(
        connection
            .sql_has_syntax_error("ALTER TABLE test ADD x TEXT")
            .is_none()
    );

    assert!(
        connection
            .sql_has_syntax_error("ALTER TABLE test AAD x TEXT")
            .is_some()
    );
}

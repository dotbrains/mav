use super::*;
use fs::FakeFs;
use gpui::TestAppContext;

#[path = "tests/builtin_tests.rs"]
mod builtin_tests;
#[path = "tests/loading_tests.rs"]
mod loading_tests;
#[path = "tests/parse_tests.rs"]
mod parse_tests;
#[path = "tests/path_tests.rs"]
mod path_tests;
#[path = "tests/share_link_tests.rs"]
mod share_link_tests;
#[path = "tests/slugify_tests.rs"]
mod slugify_tests;
#[path = "tests/source_tests.rs"]
mod source_tests;
#[path = "tests/summary_tests.rs"]
mod summary_tests;

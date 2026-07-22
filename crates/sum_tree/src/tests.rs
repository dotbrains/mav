use super::*;
#[path = "sum_tree/tests/support.rs"]
mod support;
pub(super) use support::*;
#[path = "sum_tree/tests/cursor.rs"]
mod cursor;
#[path = "sum_tree/tests/edit.rs"]
mod edit;
#[path = "sum_tree/tests/random.rs"]
mod random;
#[path = "sum_tree/tests/tree_build.rs"]
mod tree_build;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

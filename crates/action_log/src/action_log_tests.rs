#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

#[path = "action_log_tests/commit.rs"]
mod commit;
#[path = "action_log_tests/common.rs"]
mod common;
#[path = "action_log_tests/file_lifecycle.rs"]
mod file_lifecycle;
#[path = "action_log_tests/file_read_times.rs"]
mod file_read_times;
#[path = "action_log_tests/keep.rs"]
mod keep;
#[path = "action_log_tests/linked.rs"]
mod linked;
#[path = "action_log_tests/reject_basic.rs"]
mod reject_basic;
#[path = "action_log_tests/reject_created.rs"]
mod reject_created;

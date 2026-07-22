use super::*;

// Hardcoded security rules tests - these rules CANNOT be bypassed

#[test]
fn hardcoded_blocks_rm_rf_root() {
    t("rm -rf /").is_deny();
    t("rm -fr /").is_deny();
    t("rm -RF /").is_deny();
    t("rm -FR /").is_deny();
    t("rm -r -f /").is_deny();
    t("rm -f -r /").is_deny();
    t("RM -RF /").is_deny();
    t("rm /").is_deny();
    // Long flags
    t("rm --recursive --force /").is_deny();
    t("rm --force --recursive /").is_deny();
    // Extra short flags
    t("rm -rfv /").is_deny();
    t("rm -v -rf /").is_deny();
    // Glob wildcards
    t("rm -rf /*").is_deny();
    t("rm -rf /* ").is_deny();
    // End-of-options marker
    t("rm -rf -- /").is_deny();
    t("rm -- /").is_deny();
    // Prefixed with sudo or other commands
    t("sudo rm -rf /").is_deny();
    t("sudo rm -rf /*").is_deny();
    t("sudo rm -rf --no-preserve-root /").is_deny();
}

#[test]
fn hardcoded_blocks_rm_rf_home() {
    t("rm -rf ~").is_deny();
    t("rm -fr ~").is_deny();
    t("rm -rf ~/").is_deny();
    t("rm -rf $HOME").is_deny();
    t("rm -fr $HOME").is_deny();
    t("rm -rf $HOME/").is_deny();
    t("rm -rf ${HOME}").is_deny();
    t("rm -rf ${HOME}/").is_deny();
    t("rm -RF $HOME").is_deny();
    t("rm -FR ${HOME}/").is_deny();
    t("rm -R -F ${HOME}/").is_deny();
    t("RM -RF ~").is_deny();
    // Long flags
    t("rm --recursive --force ~").is_deny();
    t("rm --recursive --force ~/").is_deny();
    t("rm --recursive --force $HOME").is_deny();
    t("rm --force --recursive ${HOME}/").is_deny();
    // Extra short flags
    t("rm -rfv ~").is_deny();
    t("rm -v -rf ~/").is_deny();
    // Glob wildcards
    t("rm -rf ~/*").is_deny();
    t("rm -rf $HOME/*").is_deny();
    t("rm -rf ${HOME}/*").is_deny();
    // End-of-options marker
    t("rm -rf -- ~").is_deny();
    t("rm -rf -- ~/").is_deny();
    t("rm -rf -- $HOME").is_deny();
}

#[test]
fn hardcoded_blocks_rm_rf_home_with_traversal() {
    // Path traversal after $HOME / ${HOME} should still be blocked
    t("rm -rf $HOME/./").is_deny();
    t("rm -rf $HOME/foo/..").is_deny();
    t("rm -rf ${HOME}/.").is_deny();
    t("rm -rf ${HOME}/./").is_deny();
    t("rm -rf $HOME/a/b/../..").is_deny();
    t("rm -rf ${HOME}/foo/bar/../..").is_deny();
    // Subdirectories should NOT be blocked
    t("rm -rf $HOME/subdir")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf ${HOME}/Documents")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn hardcoded_blocks_rm_rf_dot() {
    t("rm -rf .").is_deny();
    t("rm -fr .").is_deny();
    t("rm -rf ./").is_deny();
    t("rm -rf ..").is_deny();
    t("rm -fr ..").is_deny();
    t("rm -rf ../").is_deny();
    t("rm -RF .").is_deny();
    t("rm -FR ../").is_deny();
    t("rm -R -F ../").is_deny();
    t("RM -RF .").is_deny();
    t("RM -RF ..").is_deny();
    // Long flags
    t("rm --recursive --force .").is_deny();
    t("rm --force --recursive ../").is_deny();
    // Extra short flags
    t("rm -rfv .").is_deny();
    t("rm -v -rf ../").is_deny();
    // Glob wildcards
    t("rm -rf ./*").is_deny();
    t("rm -rf ../*").is_deny();
    // End-of-options marker
    t("rm -rf -- .").is_deny();
    t("rm -rf -- ../").is_deny();
}

#[test]
fn hardcoded_cannot_be_bypassed_by_global() {
    // Even with global default Allow, hardcoded rules block
    t("rm -rf /")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("rm -rf ~")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("rm -rf $HOME")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("rm -rf .")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("rm -rf ..")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
}

#[test]
fn hardcoded_cannot_be_bypassed_by_allow_pattern() {
    // Even with an allow pattern that matches, hardcoded rules block
    t("rm -rf /").allow(&[".*"]).is_deny();
    t("rm -rf $HOME").allow(&[".*"]).is_deny();
    t("rm -rf .").allow(&[".*"]).is_deny();
    t("rm -rf ..").allow(&[".*"]).is_deny();
}

#[test]
fn hardcoded_allows_safe_rm() {
    // rm -rf on a specific path should NOT be blocked
    t("rm -rf ./build")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf /tmp/test")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf ~/Documents")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf $HOME/Documents")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf ../some_dir")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf .hidden_dir")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rfv ./build")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm --recursive --force ./build")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn hardcoded_checks_chained_commands() {
    // Hardcoded rules should catch dangerous commands in chains
    t("ls && rm -rf /").is_deny();
    t("echo hello; rm -rf ~").is_deny();
    t("cargo build && rm -rf /")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("echo hello; rm -rf $HOME").is_deny();
    t("echo hello; rm -rf .").is_deny();
    t("echo hello; rm -rf ..").is_deny();
}

#[test]
fn hardcoded_blocks_rm_with_extra_flags() {
    // Extra flags like -v, -i should not bypass the security rules
    t("rm -rfv /").is_deny();
    t("rm -v -rf /").is_deny();
    t("rm -rfi /").is_deny();
    t("rm -rfv ~").is_deny();
    t("rm -rfv ~/").is_deny();
    t("rm -rfv $HOME").is_deny();
    t("rm -rfv .").is_deny();
    t("rm -rfv ./").is_deny();
    t("rm -rfv ..").is_deny();
    t("rm -rfv ../").is_deny();
}

#[test]
fn hardcoded_blocks_rm_with_long_flags() {
    t("rm --recursive --force /").is_deny();
    t("rm --force --recursive /").is_deny();
    t("rm --recursive --force ~").is_deny();
    t("rm --recursive --force ~/").is_deny();
    t("rm --recursive --force $HOME").is_deny();
    t("rm --recursive --force .").is_deny();
    t("rm --recursive --force ..").is_deny();
}

#[test]
fn hardcoded_blocks_rm_with_glob_star() {
    // rm -rf /* is equally catastrophic to rm -rf /
    t("rm -rf /*").is_deny();
    t("rm -rf ~/*").is_deny();
    t("rm -rf $HOME/*").is_deny();
    t("rm -rf ${HOME}/*").is_deny();
    t("rm -rf ./*").is_deny();
    t("rm -rf ../*").is_deny();
}

#[test]
fn hardcoded_extra_flags_allow_safe_rm() {
    // Extra flags on specific paths should NOT be blocked
    t("rm -rfv ~/somedir")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rfv /tmp/test")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm --recursive --force ./build")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn hardcoded_does_not_block_words_containing_rm() {
    // Words like "storm", "inform" contain "rm" but should not be blocked
    t("storm -rf /").mode(ToolPermissionMode::Allow).is_allow();
    t("inform -rf /").mode(ToolPermissionMode::Allow).is_allow();
    t("gorm -rf ~").mode(ToolPermissionMode::Allow).is_allow();
}

#[test]
fn hardcoded_blocks_rm_with_trailing_flags() {
    // GNU rm accepts flags after operands by default
    t("rm / -rf").is_deny();
    t("rm / -fr").is_deny();
    t("rm / -RF").is_deny();
    t("rm / -r -f").is_deny();
    t("rm / --recursive --force").is_deny();
    t("rm / -rfv").is_deny();
    t("rm /* -rf").is_deny();
    // Mixed: some flags before path, some after
    t("rm -r / -f").is_deny();
    t("rm -f / -r").is_deny();
    // Home
    t("rm ~ -rf").is_deny();
    t("rm ~/ -rf").is_deny();
    t("rm ~ -r -f").is_deny();
    t("rm $HOME -rf").is_deny();
    t("rm ${HOME} -rf").is_deny();
    // Dot / dotdot
    t("rm . -rf").is_deny();
    t("rm ./ -rf").is_deny();
    t("rm . -r -f").is_deny();
    t("rm .. -rf").is_deny();
    t("rm ../ -rf").is_deny();
    t("rm .. -r -f").is_deny();
    // Trailing flags in chained commands
    t("ls && rm / -rf").is_deny();
    t("echo hello; rm ~ -rf").is_deny();
    // Safe paths with trailing flags should NOT be blocked
    t("rm ./build -rf")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm /tmp/test -rf")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm ~/Documents -rf")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn hardcoded_blocks_rm_with_flag_equals_value() {
    // --flag=value syntax should not bypass the rules
    t("rm --no-preserve-root=yes -rf /").is_deny();
    t("rm --no-preserve-root=yes --recursive --force /").is_deny();
    t("rm -rf --no-preserve-root=yes /").is_deny();
    t("rm --interactive=never -rf /").is_deny();
    t("rm --no-preserve-root=yes -rf ~").is_deny();
    t("rm --no-preserve-root=yes -rf .").is_deny();
    t("rm --no-preserve-root=yes -rf ..").is_deny();
    t("rm --no-preserve-root=yes -rf $HOME").is_deny();
    // --flag (without =value) should also not bypass the rules
    t("rm -rf --no-preserve-root /").is_deny();
    t("rm --no-preserve-root -rf /").is_deny();
    t("rm --no-preserve-root --recursive --force /").is_deny();
    t("rm -rf --no-preserve-root ~").is_deny();
    t("rm -rf --no-preserve-root .").is_deny();
    t("rm -rf --no-preserve-root ..").is_deny();
    t("rm -rf --no-preserve-root $HOME").is_deny();
    // Trailing --flag=value after path
    t("rm / --no-preserve-root=yes -rf").is_deny();
    t("rm ~ -rf --no-preserve-root=yes").is_deny();
    // Trailing --flag (without =value) after path
    t("rm / -rf --no-preserve-root").is_deny();
    t("rm ~ -rf --no-preserve-root").is_deny();
    // Safe paths with --flag=value should NOT be blocked
    t("rm --no-preserve-root=yes -rf ./build")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm --interactive=never -rf /tmp/test")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    // Safe paths with --flag (without =value) should NOT be blocked
    t("rm --no-preserve-root -rf ./build")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn hardcoded_blocks_rm_with_path_traversal() {
    // Traversal to root via ..
    t("rm -rf /etc/../").is_deny();
    t("rm -rf /tmp/../../").is_deny();
    t("rm -rf /tmp/../..").is_deny();
    t("rm -rf /var/log/../../").is_deny();
    // Root via /./
    t("rm -rf /./").is_deny();
    t("rm -rf /.").is_deny();
    // Double slash (equivalent to /)
    t("rm -rf //").is_deny();
    // Home traversal via ~/./
    t("rm -rf ~/./").is_deny();
    t("rm -rf ~/.").is_deny();
    // Dot traversal via indirect paths
    t("rm -rf ./foo/..").is_deny();
    t("rm -rf ../foo/..").is_deny();
    // Traversal in chained commands
    t("ls && rm -rf /tmp/../../").is_deny();
    t("echo hello; rm -rf /./").is_deny();
    // Traversal cannot be bypassed by global or allow patterns
    t("rm -rf /tmp/../../")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("rm -rf /./").allow(&[".*"]).is_deny();
    // Safe paths with traversal should still be allowed
    t("rm -rf /tmp/../tmp/foo")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf ~/Documents/./subdir")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn hardcoded_blocks_rm_multi_path_with_dangerous_last() {
    t("rm -rf /tmp /").is_deny();
    t("rm -rf /tmp/foo /").is_deny();
    t("rm -rf /var/log ~").is_deny();
    t("rm -rf /safe $HOME").is_deny();
}

#[test]
fn hardcoded_blocks_rm_multi_path_with_dangerous_first() {
    t("rm -rf / /tmp").is_deny();
    t("rm -rf ~ /var/log").is_deny();
    t("rm -rf . /tmp/foo").is_deny();
    t("rm -rf .. /safe").is_deny();
}

#[test]
fn hardcoded_allows_rm_multi_path_all_safe() {
    t("rm -rf /tmp /home/user")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf ./build ./dist")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("rm -rf /var/log/app /tmp/cache")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn hardcoded_blocks_rm_multi_path_with_traversal() {
    t("rm -rf /safe /tmp/../../").is_deny();
    t("rm -rf /tmp/../../ /safe").is_deny();
    t("rm -rf /safe /var/log/../../").is_deny();
}

#[test]
fn hardcoded_blocks_user_reported_bypass_variants() {
    // User report: "rm -rf /etc/../" normalizes to "rm -rf /" via path traversal
    t("rm -rf /etc/../").is_deny();
    t("rm -rf /etc/..").is_deny();
    // User report: --no-preserve-root (without =value) should not bypass
    t("rm -rf --no-preserve-root /").is_deny();
    t("rm --no-preserve-root -rf /").is_deny();
    // User report: "rm -rf /*" should be caught (glob expands to all top-level entries)
    t("rm -rf /*").is_deny();
    // Chained with sudo
    t("sudo rm -rf /").is_deny();
    t("sudo rm -rf --no-preserve-root /").is_deny();
    // Traversal cannot be bypassed even with global allow or allow patterns
    t("rm -rf /etc/../")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("rm -rf /etc/../").allow(&[".*"]).is_deny();
    t("rm -rf --no-preserve-root /")
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
    t("rm -rf --no-preserve-root /").allow(&[".*"]).is_deny();
}

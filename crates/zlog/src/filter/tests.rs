use log::LevelFilter;

use crate::Scope;
use crate::private::scope_new;

use super::*;

fn scope_map_from_keys(kv: &[(&str, &str)]) -> ScopeMap {
    let hash_map: HashMap<String, String> = kv
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    ScopeMap::new_from_settings_and_env(&hash_map, None, &[])
}

#[test]
fn test_initialization() {
    let map = scope_map_from_keys(&[("a.b.c.d", "trace")]);
    assert_eq!(map.root_count, 1);
    assert_eq!(map.entries.len(), 4);

    let map = scope_map_from_keys(&[]);
    assert_eq!(map.root_count, 0);
    assert_eq!(map.entries.len(), 0);

    let map = scope_map_from_keys(&[("", "trace")]);
    assert_eq!(map.root_count, 0);
    assert_eq!(map.entries.len(), 0);

    let map = scope_map_from_keys(&[("foo..bar", "trace")]);
    assert_eq!(map.root_count, 1);
    assert_eq!(map.entries.len(), 2);

    let map = scope_map_from_keys(&[
        ("a.b.c.d", "trace"),
        ("e.f.g.h", "debug"),
        ("i.j.k.l", "info"),
        ("m.n.o.p", "warn"),
        ("q.r.s.t", "error"),
    ]);
    assert_eq!(map.root_count, 5);
    assert_eq!(map.entries.len(), 20);
    assert_eq!(map.entries[0].scope, "a");
    assert_eq!(map.entries[1].scope, "e");
    assert_eq!(map.entries[2].scope, "i");
    assert_eq!(map.entries[3].scope, "m");
    assert_eq!(map.entries[4].scope, "q");
}

fn scope_from_scope_str(scope_str: &'static str) -> Scope {
    let mut scope_buf = [""; SCOPE_DEPTH_MAX];
    let mut index = 0;
    let mut scope_iter = scope_str.split(SCOPE_STRING_SEP_STR);
    while index < SCOPE_DEPTH_MAX {
        let Some(scope) = scope_iter.next() else {
            break;
        };
        if scope.is_empty() {
            continue;
        }
        scope_buf[index] = scope;
        index += 1;
    }
    assert_ne!(index, 0);
    assert!(scope_iter.next().is_none());
    scope_buf
}

#[test]
fn test_is_enabled() {
    let map = scope_map_from_keys(&[
        ("a.b.c.d", "trace"),
        ("e.f.g.h", "debug"),
        ("i.j.k.l", "info"),
        ("m.n.o.p", "warn"),
        ("q.r.s.t", "error"),
    ]);
    use log::Level;
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("a.b.c.d"), None, Level::Trace),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("a.b.c.d"), None, Level::Debug),
        EnabledStatus::Enabled
    );

    assert_eq!(
        map.is_enabled(&scope_from_scope_str("e.f.g.h"), None, Level::Debug),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("e.f.g.h"), None, Level::Info),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("e.f.g.h"), None, Level::Trace),
        EnabledStatus::Disabled
    );

    assert_eq!(
        map.is_enabled(&scope_from_scope_str("i.j.k.l"), None, Level::Info),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("i.j.k.l"), None, Level::Warn),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("i.j.k.l"), None, Level::Debug),
        EnabledStatus::Disabled
    );

    assert_eq!(
        map.is_enabled(&scope_from_scope_str("m.n.o.p"), None, Level::Warn),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("m.n.o.p"), None, Level::Error),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("m.n.o.p"), None, Level::Info),
        EnabledStatus::Disabled
    );

    assert_eq!(
        map.is_enabled(&scope_from_scope_str("q.r.s.t"), None, Level::Error),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("q.r.s.t"), None, Level::Warn),
        EnabledStatus::Disabled
    );
}

#[test]
fn test_is_enabled_module() {
    let mut map = scope_map_from_keys(&[("a", "trace")]);
    map.modules = [("a::b::c", "trace"), ("a::b::d", "debug")]
        .map(|(k, v)| (k.to_string(), v.parse().unwrap()))
        .to_vec();
    use log::Level;
    assert_eq!(
        map.is_enabled(
            &scope_from_scope_str("__unused__"),
            Some("a::b::c"),
            Level::Trace
        ),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(
            &scope_from_scope_str("__unused__"),
            Some("a::b::d"),
            Level::Debug
        ),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(
            &scope_from_scope_str("__unused__"),
            Some("a::b::d"),
            Level::Trace,
        ),
        EnabledStatus::Disabled
    );
    assert_eq!(
        map.is_enabled(
            &scope_from_scope_str("__unused__"),
            Some("a::e"),
            Level::Info
        ),
        EnabledStatus::NotConfigured
    );
    // when scope is just crate name, more specific module path overrides it
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("a"), Some("a::b::d"), Level::Trace),
        EnabledStatus::Disabled,
    );
    // but when it is scoped, the scope overrides the module path
    assert_eq!(
        map.is_enabled(
            &scope_from_scope_str("a.scope"),
            Some("a::b::d"),
            Level::Trace
        ),
        EnabledStatus::Enabled,
    );
}

fn scope_map_from_keys_and_env(kv: &[(&str, &str)], env: &env_config::EnvFilter) -> ScopeMap {
    let hash_map: HashMap<String, String> = kv
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    ScopeMap::new_from_settings_and_env(&hash_map, Some(env), &[])
}

#[test]
fn test_initialization_with_env() {
    let env_filter = env_config::parse("a.b=debug,u=error").unwrap();
    let map = scope_map_from_keys_and_env(&[], &env_filter);
    assert_eq!(map.root_count, 2);
    assert_eq!(map.entries.len(), 3);
    assert_eq!(
        map.is_enabled(&scope_new(&["a"]), None, log::Level::Debug),
        EnabledStatus::NotConfigured
    );
    assert_eq!(
        map.is_enabled(&scope_new(&["a", "b"]), None, log::Level::Debug),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_new(&["a", "b", "c"]), None, log::Level::Trace),
        EnabledStatus::Disabled
    );

    let env_filter = env_config::parse("a.b=debug,e.f.g.h=trace,u=error").unwrap();
    let map = scope_map_from_keys_and_env(
        &[
            ("a.b.c.d", "trace"),
            ("e.f.g.h", "debug"),
            ("i.j.k.l", "info"),
            ("m.n.o.p", "warn"),
            ("q.r.s.t", "error"),
        ],
        &env_filter,
    );
    assert_eq!(map.root_count, 6);
    assert_eq!(map.entries.len(), 21);
    assert_eq!(map.entries[0].scope, "a");
    assert_eq!(map.entries[1].scope, "e");
    assert_eq!(map.entries[2].scope, "i");
    assert_eq!(map.entries[3].scope, "m");
    assert_eq!(map.entries[4].scope, "q");
    assert_eq!(map.entries[5].scope, "u");
    assert_eq!(
        map.is_enabled(&scope_new(&["a", "b", "c", "d"]), None, log::Level::Trace),
        EnabledStatus::Enabled
    );
    assert_eq!(
        map.is_enabled(&scope_new(&["a", "b", "c"]), None, log::Level::Trace),
        EnabledStatus::Disabled
    );
    assert_eq!(
        map.is_enabled(&scope_new(&["u", "v"]), None, log::Level::Warn),
        EnabledStatus::Disabled
    );
    // settings override env
    assert_eq!(
        map.is_enabled(&scope_new(&["e", "f", "g", "h"]), None, log::Level::Trace),
        EnabledStatus::Disabled,
    );
}

fn scope_map_from_all(
    kv: &[(&str, &str)],
    env: &env_config::EnvFilter,
    default_filters: &[(&str, log::LevelFilter)],
) -> ScopeMap {
    let hash_map: HashMap<String, String> = kv
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    ScopeMap::new_from_settings_and_env(&hash_map, Some(env), default_filters)
}

#[test]
fn precedence() {
    // Test precedence: kv > env > default

    // Default filters - these should be overridden by env and kv when they overlap
    let default_filters = &[
        ("a.b.c", log::LevelFilter::Debug), // Should be overridden by env
        ("p.q.r", log::LevelFilter::Info),  // Should be overridden by kv
        ("x.y.z", log::LevelFilter::Warn),  // Not overridden
        ("crate::module::default", log::LevelFilter::Error), // Module in default
        ("crate::module::user", log::LevelFilter::Off), // Module disabled in default
    ];

    // Environment filters - these should override default but be overridden by kv
    let env_filter =
        env_config::parse("a.b.c=trace,p.q=debug,m.n.o=error,crate::module::env=debug").unwrap();

    // Key-value filters (highest precedence) - these should override everything
    let kv_filters = &[
        ("p.q.r", "trace"),              // Overrides default
        ("m.n.o", "warn"),               // Overrides env
        ("j.k.l", "info"),               // New filter
        ("crate::module::env", "trace"), // Overrides env for module
        ("crate::module::kv", "trace"),  // New module filter
    ];

    let map = scope_map_from_all(kv_filters, &env_filter, default_filters);

    // Test scope precedence
    use log::Level;

    // KV overrides all for scopes
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("p.q.r"), None, Level::Trace),
        EnabledStatus::Enabled,
        "KV should override default filters for scopes"
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("m.n.o"), None, Level::Warn),
        EnabledStatus::Enabled,
        "KV should override env filters for scopes"
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("m.n.o"), None, Level::Debug),
        EnabledStatus::Disabled,
        "KV correctly limits log level"
    );

    // ENV overrides default but not KV for scopes
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("a.b.c"), None, Level::Trace),
        EnabledStatus::Enabled,
        "ENV should override default filters for scopes"
    );

    // Default is used when no override exists for scopes
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("x.y.z"), None, Level::Warn),
        EnabledStatus::Enabled,
        "Default filters should work when not overridden"
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("x.y.z"), None, Level::Info),
        EnabledStatus::Disabled,
        "Default filters correctly limit log level"
    );

    // KV overrides all for modules
    assert_eq!(
        map.is_enabled(&scope_new(&[""]), Some("crate::module::env"), Level::Trace),
        EnabledStatus::Enabled,
        "KV should override env filters for modules"
    );
    assert_eq!(
        map.is_enabled(&scope_new(&[""]), Some("crate::module::kv"), Level::Trace),
        EnabledStatus::Enabled,
        "KV module filters should work"
    );

    // ENV overrides default for modules
    assert_eq!(
        map.is_enabled(&scope_new(&[""]), Some("crate::module::env"), Level::Debug),
        EnabledStatus::Enabled,
        "ENV should override default for modules"
    );

    // Default is used when no override exists for modules
    assert_eq!(
        map.is_enabled(
            &scope_new(&[""]),
            Some("crate::module::default"),
            Level::Error
        ),
        EnabledStatus::Enabled,
        "Default filters should work for modules"
    );
    assert_eq!(
        map.is_enabled(
            &scope_new(&[""]),
            Some("crate::module::default"),
            Level::Warn
        ),
        EnabledStatus::Disabled,
        "Default filters correctly limit log level for modules"
    );

    assert_eq!(
        map.is_enabled(&scope_new(&[""]), Some("crate::module::user"), Level::Error),
        EnabledStatus::Disabled,
        "Module turned off in default filters is not enabled"
    );

    assert_eq!(
        map.is_enabled(
            &scope_new(&["crate"]),
            Some("crate::module::user"),
            Level::Error
        ),
        EnabledStatus::Disabled,
        "Module turned off in default filters is not enabled, even with crate name as scope"
    );

    // Test non-conflicting but similar paths

    // Test that "a.b" and "a.b.c" don't conflict (different depth)
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("a.b.c.d"), None, Level::Trace),
        EnabledStatus::Enabled,
        "Scope a.b.c should inherit from a.b env filter"
    );
    assert_eq!(
        map.is_enabled(&scope_from_scope_str("a.b.c"), None, Level::Trace),
        EnabledStatus::Enabled,
        "Scope a.b.c.d should use env filter level (trace)"
    );

    // Test that similar module paths don't conflict
    assert_eq!(
        map.is_enabled(&scope_new(&[""]), Some("crate::module"), Level::Error),
        EnabledStatus::NotConfigured,
        "Module crate::module should not be affected by crate::module::default filter"
    );
    assert_eq!(
        map.is_enabled(
            &scope_new(&[""]),
            Some("crate::module::default::sub"),
            Level::Error
        ),
        EnabledStatus::NotConfigured,
        "Module crate::module::default::sub should not be affected by crate::module::default filter"
    );
}

#[test]
fn default_filter_crate() {
    let default_filters = &[("crate", LevelFilter::Off)];
    let map = scope_map_from_all(&[], &env_config::parse("").unwrap(), default_filters);

    use log::Level;
    assert_eq!(
        map.is_enabled(&scope_new(&[""]), Some("crate::submodule"), Level::Error),
        EnabledStatus::Disabled,
        "crate::submodule should be disabled by disabling `crate` filter"
    );
}

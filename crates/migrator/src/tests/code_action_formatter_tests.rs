use super::*;

#[test]
fn test_flatten_code_action_formatters_basic_array() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_01::flatten_code_actions_formatters,
        )],
        &r#"{
        "formatter": [
          {
            "code_actions": {
              "included-1": true,
              "included-2": true,
              "excluded": false,
            }
          }
        ]
      }"#
        .unindent(),
        Some(
            &r#"{
        "formatter": [
          {
            "code_action": "included-1"
          },
          {
            "code_action": "included-2"
          }
        ]
      }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_basic_object() {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_01::flatten_code_actions_formatters,
        )],
        &r#"{
        "formatter": {
          "code_actions": {
            "included-1": true,
            "excluded": false,
            "included-2": true
          }
        }
      }"#
        .unindent(),
        Some(
            &r#"{
                  "formatter": [
                    {
                      "code_action": "included-1"
                    },
                    {
                      "code_action": "included-2"
                    }
                  ]
                }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_array_with_multiple_action_blocks() {
    assert_migrate_settings(
        &r#"{
          "formatter": [
            {
               "code_actions": {
                  "included-1": true,
                  "included-2": true,
                  "excluded": false,
               }
            },
            {
              "language_server": "ruff"
            },
            {
               "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
               }
            }
            // some comment
            ,
            {
               "code_actions": {
                "excluded": false,
                "included-3": true,
                "included-4": true,
               }
            },
          ]
        }"#
        .unindent(),
        Some(
            &r#"{
        "formatter": [
          {
            "code_action": "included-1"
          },
          {
            "code_action": "included-2"
          },
          {
            "language_server": "ruff"
          },
          {
            "code_action": "included-3"
          },
          {
            "code_action": "included-4"
          }
        ]
      }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_array_with_multiple_action_blocks_in_languages() {
    assert_migrate_settings(
        &r#"{
        "languages": {
          "Rust": {
            "formatter": [
              {
                "code_actions": {
                  "included-1": true,
                  "included-2": true,
                  "excluded": false,
                }
              },
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          }
        }
      }"#
        .unindent(),
        Some(
            &r#"{
          "languages": {
            "Rust": {
              "formatter": [
                {
                  "code_action": "included-1"
                },
                {
                  "code_action": "included-2"
                },
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            }
          }
        }"#
            .unindent(),
        ),
    );
}

#[test]
fn test_flatten_code_action_formatters_array_with_multiple_action_blocks_in_defaults_and_multiple_languages()
 {
    assert_migrate_with_migrations(
        &[MigrationType::Json(
            migrations::m_2025_10_01::flatten_code_actions_formatters,
        )],
        &r#"{
        "formatter": {
          "code_actions": {
            "default-1": true,
            "default-2": true,
            "default-3": true,
            "default-4": true,
          }
        },
        "languages": {
          "Rust": {
            "formatter": [
              {
                "code_actions": {
                  "included-1": true,
                  "included-2": true,
                  "excluded": false,
                }
              },
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          },
          "Python": {
            "formatter": [
              {
                "language_server": "ruff"
              },
              {
                "code_actions": {
                  "excluded": false,
                  "excluded-2": false,
                }
              }
              // some comment
              ,
              {
                "code_actions": {
                  "excluded": false,
                  "included-3": true,
                  "included-4": true,
                }
              },
            ]
          }
        }
      }"#
        .unindent(),
        Some(
            &r#"{
          "formatter": [
            {
              "code_action": "default-1"
            },
            {
              "code_action": "default-2"
            },
            {
              "code_action": "default-3"
            },
            {
              "code_action": "default-4"
            }
          ],
          "languages": {
            "Rust": {
              "formatter": [
                {
                  "code_action": "included-1"
                },
                {
                  "code_action": "included-2"
                },
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            },
            "Python": {
              "formatter": [
                {
                  "language_server": "ruff"
                },
                {
                  "code_action": "included-3"
                },
                {
                  "code_action": "included-4"
                }
              ]
            }
          }
        }"#
            .unindent(),
        ),
    );
}

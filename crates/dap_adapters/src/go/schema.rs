use serde_json::json;

pub(super) fn dap_schema() -> serde_json::Value {
    let common_properties = json!({
        "debugAdapter": {
            "enum": ["legacy", "dlv-dap"],
            "description": "Select which debug adapter to use with this configuration.",
            "default": "dlv-dap"
        },
        "stopOnEntry": {
            "type": "boolean",
            "description": "Automatically stop program after launch or attach.",
            "default": false
        },
        "showLog": {
            "type": "boolean",
            "description": "Show log output from the delve debugger. Maps to dlv's `--log` flag.",
            "default": false
        },
        "cwd": {
            "type": "string",
            "description": "Workspace relative or absolute path to the working directory of the program being debugged.",
            "default": "${MAV_WORKTREE_ROOT}"
        },
        "dlvFlags": {
            "type": "array",
            "description": "Extra flags for `dlv`. See `dlv help` for the full list of supported flags.",
            "items": {
                "type": "string"
            },
            "default": []
        },
        "port": {
            "type": "number",
            "description": "Debug server port. For remote configurations, this is where to connect.",
            "default": 2345
        },
        "host": {
            "type": "string",
            "description": "Debug server host. For remote configurations, this is where to connect.",
            "default": "127.0.0.1"
        },
        "substitutePath": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "from": {
                        "type": "string",
                        "description": "The absolute local path to be replaced."
                    },
                    "to": {
                        "type": "string",
                        "description": "The absolute remote path to replace with."
                    }
                }
            },
            "description": "Mappings from local to remote paths for debugging.",
            "default": []
        },
        "trace": {
            "type": "string",
            "enum": ["verbose", "trace", "log", "info", "warn", "error"],
            "default": "error",
            "description": "Debug logging level."
        },
        "backend": {
            "type": "string",
            "enum": ["default", "native", "lldb", "rr"],
            "description": "Backend used by delve. Maps to `dlv`'s `--backend` flag."
        },
        "logOutput": {
            "type": "string",
            "enum": ["debugger", "gdbwire", "lldbout", "debuglineerr", "rpc", "dap"],
            "description": "Components that should produce debug output.",
            "default": "debugger"
        },
        "logDest": {
            "type": "string",
            "description": "Log destination for delve."
        },
        "stackTraceDepth": {
            "type": "number",
            "description": "Maximum depth of stack traces.",
            "default": 50
        },
        "showGlobalVariables": {
            "type": "boolean",
            "default": false,
            "description": "Show global package variables in variables pane."
        },
        "showRegisters": {
            "type": "boolean",
            "default": false,
            "description": "Show register variables in variables pane."
        },
        "hideSystemGoroutines": {
            "type": "boolean",
            "default": false,
            "description": "Hide system goroutines from call stack view."
        },
        "console": {
            "default": "internalConsole",
            "description": "Where to launch the debugger.",
            "enum": ["internalConsole", "integratedTerminal"]
        },
        "asRoot": {
            "default": false,
            "description": "Debug with elevated permissions (on Unix).",
            "type": "boolean"
        }
    });

    let launch_properties = json!({
        "program": {
            "type": "string",
            "description": "Path to the program folder or file to debug.",
            "default": "${MAV_WORKTREE_ROOT}"
        },
        "args": {
            "type": ["array", "string"],
            "description": "Command line arguments for the program.",
            "items": {
                "type": "string"
            },
            "default": []
        },
        "env": {
            "type": "object",
            "description": "Environment variables for the debugged program.",
            "default": {}
        },
        "envFile": {
            "type": ["string", "array"],
            "items": {
                "type": "string"
            },
            "description": "Path(s) to files with environment variables.",
            "default": ""
        },
        "buildFlags": {
            "type": ["string", "array"],
            "items": {
                "type": "string"
            },
            "description": "Flags for the Go compiler.",
            "default": []
        },
        "output": {
            "type": "string",
            "description": "Output path for the binary.",
            "default": "debug"
        },
        "mode": {
            "enum": [ "debug", "test", "exec", "replay", "core"],
            "description": "Debug mode for launch configuration.",
        },
        "traceDirPath": {
            "type": "string",
            "description": "Directory for record trace (for 'replay' mode).",
            "default": ""
        },
        "coreFilePath": {
            "type": "string",
            "description": "Path to core dump file (for 'core' mode).",
            "default": ""
        }
    });

    let attach_properties = json!({
        "processId": {
            "anyOf": [
                {
                    "enum": ["${command:pickProcess}", "${command:pickGoProcess}"],
                    "description": "Use process picker to select a process."
                },
                {
                    "type": "string",
                    "description": "Process name to attach to."
                },
                {
                    "type": "number",
                    "description": "Process ID to attach to."
                }
            ],
            "default": 0
        },
        "mode": {
            "enum": ["local", "remote"],
            "description": "Local or remote debugging.",
            "default": "local"
        },
        "remotePath": {
            "type": "string",
            "description": "Path to source on remote machine.",
            "markdownDeprecationMessage": "Use `substitutePath` instead.",
            "default": ""
        }
    });

    json!({
        "oneOf": [
            {
                "allOf": [
                    {
                        "type": "object",
                        "required": ["request"],
                        "properties": {
                            "request": {
                                "type": "string",
                                "enum": ["launch"],
                                "description": "Request to launch a new process"
                            }
                        }
                    },
                    {
                        "type": "object",
                        "properties": common_properties
                    },
                    {
                        "type": "object",
                        "required": ["program", "mode"],
                        "properties": launch_properties
                    }
                ]
            },
            {
                "allOf": [
                    {
                        "type": "object",
                        "required": ["request"],
                        "properties": {
                            "request": {
                                "type": "string",
                                "enum": ["attach"],
                                "description": "Request to attach to an existing process"
                            }
                        }
                    },
                    {
                        "type": "object",
                        "properties": common_properties
                    },
                    {
                        "type": "object",
                        "required": ["mode"],
                        "properties": attach_properties
                    }
                ]
            }
        ]
    })
}

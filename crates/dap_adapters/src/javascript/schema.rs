use serde_json::json;

pub(super) fn dap_schema() -> serde_json::Value {
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
                        "properties": {
                            "type": {
                                "type": "string",
                                "enum": ["pwa-node", "node", "chrome", "pwa-chrome", "msedge", "pwa-msedge", "node-terminal"],
                                "description": "The type of debug session",
                                "default": "pwa-node"
                            },
                            "program": {
                                "type": "string",
                                "description": "Path to the program or file to debug"
                            },
                            "cwd": {
                                "type": "string",
                                "description": "Absolute path to the working directory of the program being debugged"
                            },
                            "args": {
                                "type": ["array", "string"],
                                "description": "Command line arguments passed to the program",
                                "items": {
                                    "type": "string"
                                },
                                "default": []
                            },
                            "env": {
                                "type": "object",
                                "description": "Environment variables passed to the program",
                                "default": {}
                            },
                            "envFile": {
                                "type": ["string", "array"],
                                "description": "Path to a file containing environment variable definitions",
                                "items": {
                                    "type": "string"
                                }
                            },
                            "stopOnEntry": {
                                "type": "boolean",
                                "description": "Automatically stop program after launch",
                                "default": false
                            },
                            "attachSimplePort": {
                                "type": "number",
                                "description": "If set, attaches to the process via the given port. This is generally no longer necessary for Node.js programs and loses the ability to debug child processes, but can be useful in more esoteric scenarios such as with Deno and Docker launches. If set to 0, a random port will be chosen and --inspect-brk added to the launch arguments automatically."
                            },
                            "runtimeExecutable": {
                                "type": ["string", "null"],
                                "description": "Runtime to use, an absolute path or the name of a runtime available on PATH",
                                "default": "node"
                            },
                            "runtimeArgs": {
                                "type": ["array", "null"],
                                "description": "Arguments passed to the runtime executable",
                                "items": {
                                    "type": "string"
                                },
                                "default": []
                            },
                            "outFiles": {
                                "type": "array",
                                "description": "Glob patterns for locating generated JavaScript files",
                                "items": {
                                    "type": "string"
                                },
                                "default": ["${MAV_WORKTREE_ROOT}/**/*.js", "!**/node_modules/**"]
                            },
                            "sourceMaps": {
                                "type": "boolean",
                                "description": "Use JavaScript source maps if they exist",
                                "default": true
                            },
                            "pauseForSourceMap": {
                                "type": "boolean",
                                "description": "Wait for source maps to load before setting breakpoints.",
                                "default": true
                            },
                            "sourceMapRenames": {
                                "type": "boolean",
                                "description": "Whether to use the \"names\" mapping in sourcemaps.",
                                "default": true
                            },
                            "sourceMapPathOverrides": {
                                "type": "object",
                                "description": "Rewrites the locations of source files from what the sourcemap says to their locations on disk",
                                "default": {}
                            },
                            "restart": {
                                "type": ["boolean", "object"],
                                "description": "Restart session after Node.js has terminated",
                                "default": false
                            },
                            "trace": {
                                "type": ["boolean", "object"],
                                "description": "Enables logging of the Debug Adapter",
                                "default": false
                            },
                            "console": {
                                "type": "string",
                                "enum": ["internalConsole", "integratedTerminal"],
                                "description": "Where to launch the debug target",
                                "default": "internalConsole"
                            },
                            "url": {
                                "type": ["string", "null"],
                                "description": "Will navigate to this URL and attach to it (browser debugging)"
                            },
                            "webRoot": {
                                "type": "string",
                                "description": "Workspace absolute path to the webserver root",
                                "default": "${MAV_WORKTREE_ROOT}"
                            },
                            "userDataDir": {
                                "type": ["string", "boolean"],
                                "description": "Path to a custom Chrome user profile (browser debugging)",
                                "default": true
                            },
                            "skipFiles": {
                                "type": "array",
                                "description": "An array of glob patterns for files to skip when debugging",
                                "items": {
                                    "type": "string"
                                },
                                "default": ["<node_internals>/**"]
                            },
                            "timeout": {
                                "type": "number",
                                "description": "Retry for this number of milliseconds to connect to the debug adapter",
                                "default": 10000
                            },
                            "resolveSourceMapLocations": {
                                "type": ["array", "null"],
                                "description": "A list of minimatch patterns for source map resolution",
                                "items": {
                                    "type": "string"
                                }
                            }
                        },
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
                        "properties": {
                            "type": {
                                "type": "string",
                                "enum": ["pwa-node", "node", "chrome", "pwa-chrome", "edge", "pwa-edge"],
                                "description": "The type of debug session",
                                "default": "pwa-node"
                            },
                            "processId": {
                                "type": ["string", "number"],
                                "description": "ID of process to attach to (Node.js debugging)"
                            },
                            "port": {
                                "type": "number",
                                "description": "Debug port to attach to",
                                "default": 9229
                            },
                            "address": {
                                "type": "string",
                                "description": "TCP/IP address of the process to be debugged",
                                "default": "localhost"
                            },
                            "restart": {
                                "type": ["boolean", "object"],
                                "description": "Restart session after Node.js has terminated",
                                "default": false
                            },
                            "sourceMaps": {
                                "type": "boolean",
                                "description": "Use JavaScript source maps if they exist",
                                "default": true
                            },
                            "sourceMapPathOverrides": {
                                "type": "object",
                                "description": "Rewrites the locations of source files from what the sourcemap says to their locations on disk",
                                "default": {}
                            },
                            "outFiles": {
                                "type": "array",
                                "description": "Glob patterns for locating generated JavaScript files",
                                "items": {
                                    "type": "string"
                                },
                                "default": ["${MAV_WORKTREE_ROOT}/**/*.js", "!**/node_modules/**"]
                            },
                            "url": {
                                "type": "string",
                                "description": "Will search for a page with this URL and attach to it (browser debugging)"
                            },
                            "webRoot": {
                                "type": "string",
                                "description": "Workspace absolute path to the webserver root",
                                "default": "${MAV_WORKTREE_ROOT}"
                            },
                            "skipFiles": {
                                "type": "array",
                                "description": "An array of glob patterns for files to skip when debugging",
                                "items": {
                                    "type": "string"
                                },
                                "default": ["<node_internals>/**"]
                            },
                            "timeout": {
                                "type": "number",
                                "description": "Retry for this number of milliseconds to connect to the debug adapter",
                                "default": 10000
                            },
                            "resolveSourceMapLocations": {
                                "type": ["array", "null"],
                                "description": "A list of minimatch patterns for source map resolution",
                                "items": {
                                    "type": "string"
                                }
                            },
                            "remoteRoot": {
                                "type": ["string", "null"],
                                "description": "Path to the remote directory containing the program"
                            },
                            "localRoot": {
                                "type": ["string", "null"],
                                "description": "Path to the local directory containing the program"
                            }
                        },
                        "oneOf": [
                            { "required": ["processId"] },
                            { "required": ["port"] }
                        ]
                    }
                ]
            }
        ]
    })
}

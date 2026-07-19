# mav-eval

Python CLI and installed-agent package for building Mav's headless `eval-cli`
binary, launching benchmark runs on Modal/Harbor/Pier, and fetching results.

This README is for `crates/eval_cli/mav_eval/`. The Rust `eval-cli` binary is
documented in [`../README.md`](../README.md). Commands below assume they are run
from the repository root unless otherwise noted.

Most benchmark users should start with `mav-eval`.

## Quick start

Install the Python CLI as an editable tool:

```sh
crates/eval_cli/script/install-mav-eval
```

This wraps `uv tool install --editable crates/eval_cli/mav_eval --force`, so the
installed `mav-eval` command tracks checkout changes without needing
`PYTHONPATH`. If you prefer not to install globally, run from the checkout with:

```sh
crates/eval_cli/script/mav-eval doctor
```

Check your setup and create the Modal volume if needed:

```sh
mav-eval doctor --create-volume
```

Deploy the Modal app once before launching runs, and again after changing the
Python orchestration code:

```sh
mav-eval deploy
```

Deploying replaces the live app and can cancel in-flight runs, so avoid running
it while evals are active.

Launch a small benchmark run from your current checkout:

```sh
mav-eval run rf --from local --n-tasks 2
```

Monitor and report:

```sh
mav-eval runs                 # recent runs launched from this machine
mav-eval status               # status for the most recent run
mav-eval logs <run-id>        # controller log
mav-eval report <run-id> --fetch
```

After a launch, the local run index remembers the run's namespace and benchmark,
so most commands only need the run id. If the run was launched elsewhere, pass
`--experiment-name` and, if needed, `--namespace`.

## Development

Run the package tests without manually setting `PYTHONPATH`:

```sh
uv run --project crates/eval_cli/mav_eval python -m unittest discover -s crates/eval_cli/mav_eval/tests
```

## Prerequisites

You need:

- access to the Modal workspace used for evals
- a Modal token secret for the controller, default: `agent-evals-modal-token`
- an LLM-provider secret for agent/judge API keys, default:
  `agent-evals-llm-providers`

The controller secret should contain:

- `MODAL_TOKEN_ID`
- `MODAL_TOKEN_SECRET`

The LLM-provider secret should contain the keys your selected models need, such as:

- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `BASETEN_API_KEY`

Defaults can be overridden globally:

```sh
mav-eval \
  --volume agent-evals \
  --api-secret agent-evals-llm-providers \
  --modal-token-secret agent-evals-modal-token \
  doctor
```

## Launch benchmarks

Use `mav-eval run` for non-interactive launches:

```sh
# One SWE-Atlas part
mav-eval run rf --from local --n-tasks 5

# Multiple benchmarks share one build when possible
mav-eval run swe-atlas terminal-bench-2.1 --from v0.210.0 --n-tasks 5 --yes

# DeepSWE runs under Pier automatically
mav-eval run deepswe --from local --n-tasks 10

# Inspect what would happen without launching
mav-eval run swe-atlas --from v0.210.0 --plan
mav-eval run deepswe --from local --dry-run
```

Supported benchmark selectors:

| Selector | Meaning | Scoring |
| --- | --- | --- |
| `swe-atlas` | `qna`, `rf`, and `tw` | LLM judge |
| `qna` / `swe-atlas-qna` | SWE-Atlas Codebase Q&A | LLM judge |
| `rf` / `swe-atlas-rf` | SWE-Atlas Refactoring | LLM judge |
| `tw` / `swe-atlas-tw` | SWE-Atlas Test Writing | LLM judge |
| `terminal-bench-2.1` / `tb21` | Terminal-Bench 2.1 | tests |
| `deepswe` | DeepSWE | tests |

For an interactive prompt, use:

```sh
mav-eval swe-atlas --interactive
```

Despite the command name, the interactive flow can launch SWE-Atlas parts,
Terminal-Bench, and DeepSWE.

## Choose source and builds

`--from` controls what source is built into `eval-cli`:

- `--from local` builds current `HEAD` plus tracked changes.
- `--from <ref/tag/sha>` builds a clean git ref, tag, or SHA.

Builds are content-addressed and reused when possible. You can also name or reuse
a build explicitly:

```sh
mav-eval build --from local
mav-eval run rf --build bld-abc123 --n-tasks 2
mav-eval builds --details
```

Untracked files are not included in builds. If they are irrelevant, opt in:

```sh
mav-eval run rf --from local --allow-untracked --n-tasks 2
```

For reproducible runs, use a clean ref/tag/SHA or require a clean tracked tree:

```sh
mav-eval run rf --from v0.210.0 --n-tasks 2
mav-eval run rf --from local --require-clean --n-tasks 2
```

## Choose models and judges

Default agent model:

- `sonnet-4.6` → `anthropic/claude-sonnet-4-6`

Common agent model examples:

```sh
mav-eval run rf --model sonnet-4.6 --n-tasks 2
mav-eval run rf --model opus-4.5 --n-tasks 2
mav-eval run rf --model baseten:kimi-k2.7-code --n-tasks 2
mav-eval run rf --model baseten:deepseek-v4-pro --n-tasks 2
```

For SWE-Atlas judge defaults, `--judge auto` uses:

- `qna`: `deepseek-v4-pro`
- `rf` and `tw`: `kimi-k2.7-code`

Override the judge when needed:

```sh
mav-eval run rf --judge leaderboard --n-tasks 2
mav-eval run rf --judge deepseek-v4-pro --judge-model deepseek-ai/DeepSeek-V4-Pro
```

Baseten presets generate the OpenAI-compatible provider settings automatically.
The sandbox secret must include `BASETEN_API_KEY`.

## Monitor, fetch, and report

Useful commands:

```sh
mav-eval runs                         # local, fast recent-run list
mav-eval list --details               # query runs on the Modal volume
mav-eval list -e swe-atlas-rf --details
mav-eval status <run-id>
mav-eval logs <run-id>
mav-eval fetch <run-id>
mav-eval report <run-id> --fetch
```

`fetch` downloads the run archive and extracts it to:

```text
~/.cache/harbor/jobs/<run-id>/
```

`report` prints pass rate plus resource usage such as tokens, tool calls, and
agent steps. Use `--json` for machine-readable output.

## Multi-benchmark suites

Launching more than one benchmark in one command groups the runs under a
`suite_id` stored on each run.

```sh
mav-eval run qna rf terminal-bench-2.1 --from local --n-tasks 2 --yes
```

Suite commands operate on all runs in that group:

```sh
mav-eval suite status <suite-id>
mav-eval suite logs <suite-id>
mav-eval suite fetch <suite-id>
```

## Rejudge a finished run

`rejudge` creates a new derived run by re-running only the judge. It does not redo
the agent's work or modify the parent run.

```sh
mav-eval rejudge <parent-run-id> --judge deepseek-v4-pro
mav-eval rejudge <parent-run-id> --judge kimi-k2.7-code --dry-run
mav-eval report <derived-run-id> --fetch
```

If the parent run is not in your local run index, pass `--experiment-name` and
possibly `--namespace`.

## Baselines

Baseline commands record and inspect baseline-of-record results for completed
clean-commit runs:

```sh
mav-eval baseline record <run-id> --experiment-name swe-atlas-rf
mav-eval baseline list
mav-eval baseline show swe-atlas-rf --model anthropic/claude-sonnet-4-6
```

## Storage model

Runs and builds live on the shared Modal volume:

```text
builds/<build-id>/
  eval-cli
  build-info.json
  source-info.json
  source.patch

runs/<namespace>/<experiment-name>/<run-id>/
  request.json
  state.json
  controller.log
  harbor-command.txt
  run-metadata.json
  harbor-job.tar.gz
  summary.json
```

Namespaces prevent accidental collisions but are not access control. Anyone with
access to the Modal workspace/volume can read run manifests, logs, patches, and
archives.

## Standalone eval-cli

For local or custom harness usage, you can build and invoke the Rust binary
directly.

Build for the current platform:

```sh
cargo build --release -p eval_cli
```

Build the Linux binary used by Harbor/Pier sandboxes:

```sh
crates/eval_cli/script/build-linux
```

Run directly:

```sh
eval-cli \
  --workdir /testbed \
  --model anthropic/claude-sonnet-4-6 \
  --instruction "Fix the bug described in..." \
  --timeout 600 \
  --output-dir /logs/agent
```

`eval-cli` reads provider API keys from environment variables and writes
`result.json`, `thread.md`, and `thread.json` to the output directory.

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Agent finished |
| 1 | Error, such as model/auth/runtime failure |
| 2 | Timeout |
| 3 | Interrupted by SIGTERM or SIGINT |

## Harbor/Pier installed agent

The Python package also exposes installed-agent classes used by the remote
orchestrator:

- `mav_eval.agent:MavAgent` for Harbor
- `mav_eval.pier_agent:MavPierAgent` for Pier

For manual Harbor experiments with a locally built Linux binary:

```sh
pip install -e crates/eval_cli/mav_eval/
crates/eval_cli/script/build-linux

harbor run -d "swebench_verified@latest" \
  --agent-import-path mav_eval.agent:MavAgent \
  --ae binary_path=target/eval-cli \
  --ae EVAL_CLI_TIMEOUT=600 \
  -m anthropic/claude-sonnet-4-6
```

from __future__ import annotations

import argparse
import sys
from typing import Any, Callable

from . import config, harness_command, run_index, source
from .common import deployed_function, modal_call_id, print_json

# Local scratch dir used only for dry-run/plan previews of harness commands.
PREVIEW_JOBS_DIR = "/tmp/agent-evals/harbor-jobs"


def benchmark_plan_entry(
    benchmark_id: str, run_request: dict[str, Any], build_request: dict[str, Any] | None
) -> dict[str, Any]:
    return {
        "benchmark": benchmark_id,
        "run_id": run_request["run_id"],
        "harness": run_request["benchmark"]["harness"],
        "model": run_request["agent_model"],
        "judge": run_request.get("judge_preset"),
        "build_id": run_request.get("build_id"),
        "will_build": build_request is not None,
        "n_tasks": run_request.get("n_tasks"),
    }


def suite_plan_entry(
    part: str, run_request: dict[str, Any], build_request: dict[str, Any] | None
) -> dict[str, Any]:
    return {
        "part": part,
        **benchmark_plan_entry(
            run_request["benchmark"]["id"], run_request, build_request
        ),
    }


def print_plan(
    summaries: list[dict[str, Any]],
    details: list[tuple[str, dict[str, Any], dict[str, Any] | None]],
    *,
    verbose: bool,
) -> None:
    print("Plan:")
    print_json(summaries)
    if verbose:
        for header, run_request, build_request in details:
            print(f"\n=== {header} ===")
            print_dry_run(run_request, build_request)


def print_dry_run(
    run_request: dict[str, Any], build_request: dict[str, Any] | None
) -> None:
    print("Run request:")
    print_json(run_request)
    if build_request:
        print("\nBuild request:")
        build_preview = dict(build_request)
        patch = build_preview.pop("patch", "")
        build_preview["patch_line_count"] = len(patch.splitlines())
        build_preview["source"] = source.public_source_info(
            build_request.get("source") or {}
        )
        print_json(build_preview)
    command = config.redacted_command(
        harness_command.build_harness_command(run_request, PREVIEW_JOBS_DIR)
    )
    print("\nHarness command:")
    print(command)


def execute_prepared_runs(
    args: argparse.Namespace,
    prepared: list[tuple[str, dict[str, Any], dict[str, Any] | None]],
    plan_entry: Callable[
        [str, dict[str, Any], dict[str, Any] | None], dict[str, Any]
    ],
) -> int:
    if args.plan:
        print_plan(
            [
                plan_entry(label, run_request, build_request)
                for label, run_request, build_request in prepared
            ],
            prepared,
            verbose=args.verbose,
        )
        return 0
    if args.dry_run:
        for label, run_request, build_request in prepared:
            print(f"\n=== {label} ===")
            print_dry_run(run_request, build_request)
        return 0

    spawned_builds: set[str] = set()
    for label, run_request, build_request in prepared:
        print(f"\n=== Launching {label} ===")
        launch_prepared_run(args, run_request, build_request, spawned_builds)
    return 0


def launch_prepared_run(
    args: argparse.Namespace,
    run_request: dict[str, Any],
    build_request: dict[str, Any] | None,
    spawned_builds: set[str] | None = None,
) -> None:
    build_function = None
    if build_request:
        run_request["source"] = source.public_source_info(build_request["source"])
        print_untracked_warning(build_request)
        build_function = deployed_function(args, "build_eval_cli")
    record_function = deployed_function(args, "create_run_record")
    controller_function = deployed_function(args, "run_controller")

    record_state = record_function.remote(run_request)
    run_index.record_run(run_request)

    print(f"Namespace:  {run_request['namespace']}")
    print(f"Experiment: {run_request['experiment_name']}")
    print(f"Run id:     {run_request['run_id']}")
    print(f"Volume:     {run_request['volume_name']}")
    print(f"Run state:  {record_state['status']}")
    print(f"Model:      {run_request['agent_model']}")
    if run_request.get("judge_preset"):
        print(f"Judge:      {run_request['judge_preset']}")
    if run_request.get("suite_id"):
        print(f"Suite:      {run_request['suite_id']}")
    if run_request.get("build_id"):
        print(f"Build id:  {run_request['build_id']}")
    if run_request.get("task_names"):
        print(f"Tasks:     {len(run_request['task_names'])} explicit task(s)")
    elif run_request.get("n_tasks"):
        print(f"Tasks:     Harbor --n-tasks {run_request['n_tasks']}")
    else:
        print("Tasks:     full dataset selection")

    build_id = run_request.get("build_id")
    if build_function is not None and build_id not in (spawned_builds or set()):
        build_call = build_function.spawn(build_request)
        print(f"Spawned build:      {modal_call_id(build_call)}")
        if spawned_builds is not None and build_id:
            spawned_builds.add(build_id)
    controller_call = controller_function.spawn(run_request)
    print(f"Spawned controller: {modal_call_id(controller_call)}")

    run_id = run_request["run_id"]
    print(
        "\nNext steps (run id alone is enough; namespace/experiment are resolved "
        "from this machine's local run index):"
    )
    print(f"  mav-eval status {run_id}")
    print(f"  mav-eval logs {run_id}")
    print(f"  mav-eval report {run_id} --fetch")


def print_untracked_warning(build_request: dict[str, Any]) -> None:
    build_source = build_request.get("source") or {}
    untracked_files = build_source.get("untracked_files") or []
    if untracked_files:
        print(
            f"Warning: proceeding with {len(untracked_files)} untracked file(s) "
            "not included in the build patch.",
            file=sys.stderr,
        )

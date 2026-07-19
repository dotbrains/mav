from __future__ import annotations

import argparse
import uuid
from typing import Any

from . import config, run_index, source
from .common import (
    default_namespace,
    deployed_function,
    modal_call_id,
    print_json,
    utc_now,
)


def build_rejudge_request(args: argparse.Namespace) -> dict[str, Any]:
    """Build the request for re-grading an existing run with a different judge.

    The positional `run_id` is the parent run; the derived run gets a new id
    under the same experiment so `report`/`list` group them together.
    """
    judge_preset = args.judge
    config.get_judge(judge_preset)
    if getattr(args, "experiment_name", None):
        experiment_name = source.sanitize_namespace(args.experiment_name)
        namespace = default_namespace(args)
    else:
        entry = run_index.lookup(args.run_id)
        if not entry:
            raise ValueError(
                f"could not locate run '{args.run_id}' in the local run index "
                f"({run_index.index_path()}). Pass --experiment-name (and "
                "--namespace if it isn't yours)."
            )
        experiment_name = source.sanitize_namespace(entry["experiment_name"])
        namespace = source.sanitize_namespace(
            getattr(args, "namespace", None) or entry["namespace"]
        )
    parent_namespace = (
        source.sanitize_namespace(args.parent_namespace)
        if getattr(args, "parent_namespace", None)
        else namespace
    )
    parent_run_id = args.run_id
    judge_slug = source.sanitize_namespace(judge_preset)
    new_run_id = (
        args.new_run_id
        or f"{parent_run_id}-rejudge-{judge_slug}-{uuid.uuid4().hex[:6]}"
    )
    return {
        "namespace": namespace,
        "experiment_name": experiment_name,
        "run_id": new_run_id,
        "parent": {
            "namespace": parent_namespace,
            "experiment_name": experiment_name,
            "run_id": parent_run_id,
        },
        "judge_preset": judge_preset,
        "judge_model": getattr(args, "judge_model", None),
        "volume_name": args.volume,
        "api_secret_name": args.api_secret,
        "created_at": utc_now(),
    }


def command_rejudge(args: argparse.Namespace) -> int:
    rejudge_request = build_rejudge_request(args)
    if getattr(args, "dry_run", False) or getattr(args, "plan", False):
        print_json(rejudge_request)
        return 0

    controller = deployed_function(args, "rejudge_controller")
    call = controller.spawn(rejudge_request)
    run_index.record_run(
        {**rejudge_request, "volume_name": args.volume, "kind": "rejudge"}
    )
    parent = rejudge_request["parent"]
    print(f"Namespace:  {rejudge_request['namespace']}")
    print(f"Experiment: {rejudge_request['experiment_name']}")
    print(
        f"Source run: {parent['namespace']}/{parent['experiment_name']}/"
        f"{parent['run_id']}"
    )
    print(f"New run id: {rejudge_request['run_id']}")
    print(f"Judge:      {rejudge_request['judge_preset']}")
    print(f"Spawned rejudge controller: {modal_call_id(call)}")
    print("\nNext steps (run id alone is enough):")
    print(f"  mav-eval status {rejudge_request['run_id']}")
    print(f"  mav-eval report {rejudge_request['run_id']} --fetch")
    return 0

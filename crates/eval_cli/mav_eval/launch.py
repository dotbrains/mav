from __future__ import annotations

import argparse
import json
import uuid
from pathlib import Path
from typing import Any

from . import benchmarks, config, run_index, source
from .builds import prepare_build_request, resolve_source, validate_build_id
from .common import (
    dedupe_preserving_order,
    default_namespace,
    deployed_function,
    modal_call_id,
    parse_parts,
    print_json,
    utc_now,
    utc_timestamp,
)
from .launch_execution import (
    benchmark_plan_entry,
    execute_prepared_runs,
    print_untracked_warning,
    suite_plan_entry,
)
from .launch_rejudge import build_rejudge_request, command_rejudge
from .volume import build_ready_on_volume


def read_task_file(path: str | None) -> list[str]:
    if not path:
        return []
    return [
        task_name
        for line in Path(path).read_text().splitlines()
        if (task_name := line.strip()) and not task_name.startswith("#")
    ]


def baseten_provider_json(
    *,
    model_id: str,
    api_url: str,
    display_name: str | None,
    max_tokens: int,
    max_output_tokens: int,
) -> str:
    provider = {
        config.BASETEN_PROVIDER_ID: {
            "api_url": api_url,
            "available_models": [
                {
                    "name": model_id,
                    "display_name": display_name or model_id,
                    "max_tokens": max_tokens,
                    "max_output_tokens": max_output_tokens,
                    "capabilities": {
                        "tools": True,
                        "images": False,
                        "parallel_tool_calls": False,
                        "prompt_cache_key": False,
                    },
                }
            ],
        }
    }
    return json.dumps(provider, separators=(",", ":"))


def resolve_model_preset(model: str) -> str:
    resolved = config.resolve_model_preset(model)
    if resolved != model:
        return resolved
    if model.startswith("baseten:"):
        return f"{config.BASETEN_PROVIDER_ID}/{model.split(':', 1)[1]}"
    return model


def resolve_model_options(
    args: argparse.Namespace,
) -> tuple[str, str | None, list[str]]:
    raw_model = getattr(args, "model", None) or config.DEFAULT_MODEL
    model = resolve_model_preset(raw_model)
    openai_compatible_provider_json = getattr(
        args, "openai_compatible_provider_json", None
    )
    extra_api_secrets = list(getattr(args, "extra_api_secret", None) or [])

    if getattr(args, "model_provider", None) == "baseten":
        baseten_model = getattr(args, "baseten_model", None) or model
        if baseten_model.startswith(f"{config.BASETEN_PROVIDER_ID}/"):
            baseten_model = baseten_model.split("/", 1)[1]
        if baseten_model.startswith("baseten:"):
            baseten_model = baseten_model.split(":", 1)[1]
        model = f"{config.BASETEN_PROVIDER_ID}/{baseten_model}"

    if model.startswith(f"{config.BASETEN_PROVIDER_ID}/"):
        baseten_model = model.split("/", 1)[1]
        if not openai_compatible_provider_json:
            openai_compatible_provider_json = baseten_provider_json(
                model_id=baseten_model,
                api_url=getattr(args, "baseten_api_url", config.BASETEN_API_URL),
                display_name=getattr(args, "baseten_model_display_name", None),
                max_tokens=getattr(
                    args, "baseten_model_max_tokens", config.BASETEN_DEFAULT_MAX_TOKENS
                ),
                max_output_tokens=getattr(
                    args,
                    "baseten_model_max_output_tokens",
                    config.BASETEN_DEFAULT_MAX_OUTPUT_TOKENS,
                ),
            )

    return model, openai_compatible_provider_json, extra_api_secrets


def derive_run_id(
    base_run_id: str | None, suite_id: str | None, suffix: str, index: int
) -> str:
    """Pick a run id for one leg of a (possibly multi-target) launch.

    An explicit --run-id is used verbatim for the first leg and suffixed for the
    rest; otherwise legs hang off the suite id, falling back to a timestamped id
    for a lone run.
    """
    if base_run_id:
        return base_run_id if index == 0 else f"{base_run_id}-{suffix}"
    if suite_id:
        return f"{suite_id}-{suffix}-{uuid.uuid4().hex[:6]}"
    return f"{utc_timestamp()}-{uuid.uuid4().hex[:6]}"


def mint_suite_id(args: argparse.Namespace, run_count: int) -> str | None:
    """A suite id groups multiple runs from one invocation; None for a lone run."""
    if run_count <= 1:
        return None
    explicit = getattr(args, "suite_id", None)
    if explicit:
        return explicit
    prefix = getattr(args, "run_id", None) or "run"
    return f"{source.sanitize_namespace(prefix)}-{utc_timestamp()}"


def common_run_request_fields(
    args: argparse.Namespace,
    *,
    namespace: str,
    run_id: str,
    experiment_name: str,
    judge_preset: str | None,
    build_id: str | None,
    suite_id: str | None,
) -> dict[str, Any]:
    """Fields shared by every benchmark run request."""
    agent_model, openai_compatible_provider_json, extra_api_secrets = (
        resolve_model_options(args)
    )
    task_names = dedupe_preserving_order(
        read_task_file(getattr(args, "tasks", None))
        + (getattr(args, "include_task_name", None) or [])
    )
    return {
        "created_at": utc_now(),
        "namespace": namespace,
        "run_id": run_id,
        "experiment_name": experiment_name,
        "volume_name": args.volume,
        "api_secret_name": args.api_secret,
        "modal_token_secret_name": args.modal_token_secret,
        "orchestration": config.orchestration_info(),
        "agent_model": agent_model,
        "judge_preset": judge_preset,
        "judge_model": getattr(args, "judge_model", None),
        "build_id": build_id,
        "task_names": task_names,
        "n_tasks": getattr(args, "n_tasks", None),
        "n_concurrent": getattr(args, "n_concurrent", config.DEFAULT_N_CONCURRENT),
        "override_cpus": getattr(args, "override_cpus", config.DEFAULT_OVERRIDE_CPUS),
        "override_memory_mb": getattr(
            args, "override_memory_mb", config.DEFAULT_OVERRIDE_MEMORY_MB
        ),
        "sandbox_timeout_secs": getattr(
            args, "sandbox_timeout_secs", config.DEFAULT_SANDBOX_TIMEOUT_SECS
        ),
        "sandbox_idle_timeout_secs": getattr(
            args, "sandbox_idle_timeout_secs", config.DEFAULT_SANDBOX_IDLE_TIMEOUT_SECS
        ),
        "build_wait_timeout_secs": getattr(args, "build_wait_timeout_secs", 7200),
        "extra_harbor_args": list(getattr(args, "extra_harbor_arg", None) or []),
        "openai_compatible_provider_json": openai_compatible_provider_json,
        "anthropic_available_models_json": getattr(
            args, "anthropic_available_models_json", None
        ),
        "extra_api_secrets": dedupe_preserving_order(extra_api_secrets),
        "suite_id": suite_id,
    }


def resolve_benchmark_judge(
    args: argparse.Namespace, benchmark: benchmarks.Benchmark
) -> str | None:
    if not benchmark.needs_judge:
        return None
    judge = getattr(args, "judge", None) or config.DEFAULT_JUDGE_PRESET
    if judge == "auto":
        return benchmark.default_judge or "leaderboard"
    config.get_judge(judge)
    return judge


def benchmark_metadata_for_run(
    args: argparse.Namespace, benchmark: benchmarks.Benchmark
) -> dict[str, object]:
    metadata = benchmarks.benchmark_metadata(benchmark)
    dataset = metadata.get("dataset")
    if (
        isinstance(dataset, dict)
        and dataset.get("repo_url") == benchmarks.SWE_ATLAS_REPO_URL
    ):
        dataset["repo_url"] = (
            getattr(args, "swe_atlas_repo_url", None) or benchmarks.SWE_ATLAS_REPO_URL
        )
        dataset["repo_ref"] = (
            getattr(args, "swe_atlas_repo_ref", None) or benchmarks.SWE_ATLAS_REPO_REF
        )
    return metadata


def prepare_shared_build(
    args: argparse.Namespace,
) -> tuple[str | None, dict[str, Any] | None]:
    """Resolve the build once for a whole (possibly multi-benchmark) run.

    Returns `(build_id, build_request)`. `build_request` is None when the target
    build already exists; otherwise the caller should spawn the build.
    """
    explicit_build_id = getattr(args, "build", None)
    validate_build_id(explicit_build_id)
    if (
        explicit_build_id
        and not getattr(args, "plan", False)
        and not getattr(args, "dry_run", False)
        and build_ready_on_volume(args, explicit_build_id)
    ):
        print(f"Reusing existing build {explicit_build_id} (already on volume)")
        return explicit_build_id, None

    base_sha, clean_source, source_label, pre_resolved = resolve_source(args)

    build_request = prepare_build_request(
        base_sha=base_sha,
        patch_path=getattr(args, "patch_path", None),
        build_id=explicit_build_id,
        allow_untracked=getattr(args, "allow_untracked", False),
        require_clean=getattr(args, "require_clean", False),
        repo_url=getattr(args, "repo_url", None),
        clean_source=clean_source,
        source_label=source_label,
        pre_resolved_base_sha=pre_resolved,
    )
    build_id = build_request["build_id"]

    if not getattr(args, "plan", False) and not getattr(args, "dry_run", False):
        if build_ready_on_volume(args, build_id):
            print(f"Reusing existing build {build_id} (already on volume)")
            return build_id, None

    return build_id, build_request


def command_build(args: argparse.Namespace) -> int:
    build_id, build_request = prepare_shared_build(args)
    if build_request is None:
        return 0

    print(f"Build id: {build_id}")
    print(f"Base sha: {build_request['base_sha']}")
    print(f"Patch sha256: {build_request['patch_sha256'] or '(none)'}")
    print_untracked_warning(build_request)

    build_function = deployed_function(args, "build_eval_cli")
    if args.detach:
        call = build_function.spawn(build_request)
        print(f"Spawned build: {modal_call_id(call)}")
    else:
        result = build_function.remote(build_request)
        print_json(result)
    return 0


def build_benchmark_run_request(
    args: argparse.Namespace,
    *,
    benchmark_id: str,
    build_id: str | None,
    suite_id: str | None,
    index: int,
    run_id_suffix: str | None = None,
) -> dict[str, Any]:
    benchmark = benchmarks.get_benchmark(benchmark_id)
    run_id = derive_run_id(
        getattr(args, "run_id", None), suite_id, run_id_suffix or benchmark_id, index
    )
    # Staff mode is off by default for remote runs: it enables the sandboxed
    # terminal, which hangs inside Modal sandboxes.
    extra_env = {"EVAL_CLI_STAFF": "true" if getattr(args, "staff", False) else "false"}

    return {
        **common_run_request_fields(
            args,
            namespace=default_namespace(args),
            run_id=run_id,
            # The benchmark id doubles as the experiment name for run storage
            # paths (runs/<namespace>/<experiment_name>/<run_id>), keeping
            # monitoring and fetch uniform across benchmarks.
            experiment_name=source.sanitize_namespace(benchmark_id),
            judge_preset=resolve_benchmark_judge(args, benchmark),
            build_id=build_id,
            suite_id=suite_id,
        ),
        "benchmark": benchmark_metadata_for_run(args, benchmark),
        "eval_cli_timeout": getattr(args, "eval_cli_timeout", None),
        "extra_env": extra_env,
    }


def prepare_runs_for_benchmarks(
    args: argparse.Namespace,
    benchmark_ids: list[str],
    *,
    suite_id: str | None,
    label_for_benchmark,
    mark_swe_atlas_parts: bool = False,
) -> list[tuple[str, dict[str, Any], dict[str, Any] | None]]:
    if not benchmark_ids:
        raise ValueError("choose at least one benchmark to run")

    build_id, build_request = prepare_shared_build(args)
    prepared: list[tuple[str, dict[str, Any], dict[str, Any] | None]] = []
    for index, benchmark_id in enumerate(benchmark_ids):
        label = label_for_benchmark(benchmark_id)
        run_request = build_benchmark_run_request(
            args,
            benchmark_id=benchmark_id,
            build_id=build_id,
            suite_id=suite_id,
            index=index,
            run_id_suffix=label,
        )
        if (
            mark_swe_atlas_parts
            and benchmark_id in benchmarks.SWE_ATLAS_PART_BENCHMARKS.values()
        ):
            run_request["suite_part"] = label
        # Only the first run carries the build_request; launch_prepared_run
        # dedups the actual spawn via spawned_builds anyway.
        prepared.append((label, run_request, build_request if index == 0 else None))
    return prepared


def prepare_benchmark_runs(
    args: argparse.Namespace,
) -> list[tuple[str, dict[str, Any], dict[str, Any] | None]]:
    benchmark_ids = benchmarks.resolve_benchmarks(
        list(getattr(args, "benchmark", None) or [])
    )
    return prepare_runs_for_benchmarks(
        args,
        benchmark_ids,
        suite_id=mint_suite_id(args, len(benchmark_ids)),
        label_for_benchmark=lambda benchmark_id: benchmark_id,
    )


def command_run(args: argparse.Namespace) -> int:
    return execute_prepared_runs(
        args,
        prepare_benchmark_runs(args),
        benchmark_plan_entry,
    )


def resolve_suite_parts(args: argparse.Namespace) -> list[str]:
    parts = parse_parts([args.parts] if getattr(args, "parts", None) else [])
    if parts:
        return parts
    raise ValueError(
        "choose at least one SWE-Atlas part with --parts (e.g. --parts rf,qna or --parts all)"
    )


def suite_entry_label(benchmark_id: str) -> str:
    for part, part_benchmark_id in benchmarks.SWE_ATLAS_PART_BENCHMARKS.items():
        if part_benchmark_id == benchmark_id:
            return part
    return benchmark_id


def prepare_benchmark_suite(
    args: argparse.Namespace, selectors: list[str]
) -> list[tuple[str, dict[str, Any], dict[str, Any] | None]]:
    benchmark_ids = benchmarks.resolve_benchmarks(selectors)
    if not benchmark_ids:
        raise ValueError("choose at least one benchmark to run")
    if getattr(args, "mav_version", None):
        args.clean_source = True
        args.require_clean = True
    timestamp = utc_timestamp()
    prefix_seed = args.run_id_prefix or args.experiment_prefix or "swe-atlas"
    if getattr(args, "mav_version", None):
        prefix_seed = f"{prefix_seed}-{source.sanitize_namespace(args.mav_version)}"
    suite_id = args.suite_id or f"{source.sanitize_namespace(prefix_seed)}-{timestamp}"

    return prepare_runs_for_benchmarks(
        args,
        benchmark_ids,
        suite_id=suite_id,
        label_for_benchmark=suite_entry_label,
        mark_swe_atlas_parts=True,
    )


def prepare_suite(
    args: argparse.Namespace,
) -> list[tuple[str, dict[str, Any], dict[str, Any] | None]]:
    return prepare_benchmark_suite(args, resolve_suite_parts(args))


def command_swe_atlas(args: argparse.Namespace) -> int:
    from .interactive import configure_interactive_suite

    configure_interactive_suite(args)
    prepared = (
        prepare_benchmark_suite(args, args.benchmark)
        if getattr(args, "benchmark", None)
        else prepare_suite(args)
    )
    return execute_prepared_runs(args, prepared, suite_plan_entry)

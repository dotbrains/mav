use super::*;

/// Shell script that resolves and existence-checks paths in a single WSL
/// round-trip. Arguments come in triples `(kind, path, fallback)`: kind `W`
/// is a native Windows path to translate with `wslpath -u` (falling back to
/// the precomputed `/mnt/<drive>/...` mapping when translation fails), kind
/// `L` is an already-Linux path with an empty fallback. One result line is
/// printed per triple: `<ok|fallback> <ok|missing> <resolved path>`.
const PATH_RESOLUTION_SCRIPT: &str = "\
    while [ \"$#\" -ge 3 ]; do \
        kind=$1; path=$2; fallback=$3; shift 3; translate=ok; \
        if [ \"$kind\" = W ]; then \
            resolved=$(wslpath -u \"$path\" 2>/dev/null) || { resolved=$fallback; translate=fallback; }; \
        else resolved=$path; fi; \
        exists=ok; [ -e \"$resolved\" ] || exists=missing; \
        printf '%s %s %s\\n' \"$translate\" \"$exists\" \"$resolved\"; \
    done";

/// A line of [`PATH_RESOLUTION_SCRIPT`] output, parsed.
#[derive(Debug, Eq, PartialEq)]
struct ResolvedPath {
    path: String,
    used_fallback: bool,
    exists: bool,
}

/// Resolve path mappings into final WSL paths and confirm required paths exist.
/// Native drive-letter paths are translated with `wslpath -u` inside the
/// chosen distro so its actual automount configuration is honored, falling
/// back to the structural `/mnt/<drive>` mapping when translation fails
/// (e.g. a distro without `wslpath`); a wrong fallback is still caught by
/// the existence check.
///
/// Successful resolutions are memoized per `(distro, mapping)` for the life
/// of the process, so a steady-state command whose paths have all been seen
/// before resolves with zero `wsl.exe` round-trips; at most one round-trip
/// handles all cache misses ([`resolve_uncached_paths`]). A hit reuses the
/// translation — which only changes if the distro's automount configuration
/// is edited and the distro restarted — and also skips the WSL-side
/// existence re-check. That staleness is acceptable: if a cached path
/// disappears mid-session bwrap fails closed on the missing bind source rather
/// than running the command unsandboxed. Optional missing paths are not cached,
/// so a protected Git path can be created and then included by a later command.
///
/// Each mapping is paired with a human-readable description used in errors and
/// a flag for whether the path is required to exist. The returned paths are in
/// the same order as `mappings`; optional missing paths are returned as `None`.
async fn resolve_paths(
    wsl_exe: &Path,
    distro: Option<&str>,
    mappings: &[(PathMapping, &str, bool)],
) -> Result<Vec<Option<String>>> {
    type ResolutionCache = HashMap<Option<String>, HashMap<PathMapping, String>>;
    static CACHE: OnceLock<Mutex<ResolutionCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);

    let distro_key = distro.map(str::to_string);
    let mut resolved: Vec<Option<Option<String>>> = {
        let cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let per_distro = cache.get(&distro_key);
        mappings
            .iter()
            .map(|(mapping, _, _)| {
                per_distro
                    .and_then(|cached| cached.get(mapping))
                    .cloned()
                    .map(Some)
            })
            .collect()
    };

    let misses: Vec<usize> = (0..mappings.len())
        .filter(|&index| resolved[index].is_none())
        .collect();
    if !misses.is_empty() {
        let miss_mappings: Vec<&(PathMapping, &str, bool)> =
            misses.iter().map(|&index| &mappings[index]).collect();
        let miss_resolved = resolve_uncached_paths(wsl_exe, distro, &miss_mappings).await?;
        let mut cache = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let per_distro = cache.entry(distro_key).or_default();
        for (&index, path) in misses.iter().zip(miss_resolved) {
            if let Some(path) = &path {
                per_distro.insert(mappings[index].0.clone(), path.clone());
            }
            resolved[index] = Some(path);
        }
    }

    let mut paths = Vec::with_capacity(resolved.len());
    for path in resolved {
        paths.push(path.context("bug: a path mapping was left unresolved")?);
    }
    Ok(paths)
}

/// Resolve and existence-check mappings that weren't in the cache, in a
/// single `wsl.exe` round-trip. A non-login shell runs the script so profile
/// scripts can't pollute the stdout protocol.
async fn resolve_uncached_paths(
    wsl_exe: &Path,
    distro: Option<&str>,
    mappings: &[&(PathMapping, &str, bool)],
) -> Result<Vec<Option<String>>> {
    let mut args = vec![
        "--exec".to_string(),
        "sh".to_string(),
        "-c".to_string(),
        PATH_RESOLUTION_SCRIPT.to_string(),
        // argv[0] for the script; the path triples follow as "$@".
        "mav-resolve-paths".to_string(),
    ];
    args.extend(path_resolution_args(
        mappings.iter().map(|mapping| &mapping.0),
    ));
    let output = run_wsl_command(wsl_exe, distro, &args, "resolve sandbox paths").await?;
    if !output.status.success() {
        return Err(unavailable(format!(
            "failed to resolve sandbox paths in {}{}",
            wsl_distro_label(distro),
            command_failure_details(output.status.code(), &output.stderr)
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let resolved = parse_path_resolution_output(&stdout, mappings.len()).map_err(|error| {
        unavailable(format!(
            "failed to resolve sandbox paths in {}: {error:#}",
            wsl_distro_label(distro)
        ))
    })?;

    mappings
        .iter()
        .zip(resolved)
        .map(|((mapping, description, required), resolved)| {
            if resolved.used_fallback
                && let PathMapping::NativeDrive { windows_path, .. } = mapping
            {
                log::warn!(
                    "failed to translate `{windows_path}` with wslpath in {}; \
                     falling back to `{}`",
                    wsl_distro_label(distro),
                    resolved.path
                );
            }
            if !resolved.exists {
                // A bad request (the path simply isn't there), not an
                // environment problem — the model can create it or fix the path
                // and retry, so no `WSL_SANDBOX_UNAVAILABLE_PREFIX`. Protected
                // Git paths are allowed to be absent, matching Linux bwrap's
                // behavior: a missing path cannot be overlaid, so it is skipped.
                ensure!(
                    !required,
                    "mapped {description} `{}` does not exist in {}",
                    resolved.path,
                    wsl_distro_label(distro)
                );
                return Ok(None);
            }
            Ok(Some(resolved.path))
        })
        .collect()
}

/// Flatten path mappings into the `(kind, path, fallback)` argument triples
/// consumed by [`PATH_RESOLUTION_SCRIPT`].
fn path_resolution_args<'a>(mappings: impl Iterator<Item = &'a PathMapping>) -> Vec<String> {
    let mut args = Vec::new();
    for mapping in mappings {
        match mapping {
            PathMapping::Wsl(path) => {
                args.extend(["L".to_string(), path.path.clone(), String::new()]);
            }
            PathMapping::NativeDrive {
                windows_path,
                fallback,
            } => {
                args.extend(["W".to_string(), windows_path.clone(), fallback.path.clone()]);
            }
        }
    }
    args
}

/// Parse [`PATH_RESOLUTION_SCRIPT`] output: one strictly-formatted line per
/// input triple. Anything else (wrong line count, unknown status words, a
/// non-absolute path) means the stdout protocol was corrupted and is an error.
fn parse_path_resolution_output(stdout: &str, expected: usize) -> Result<Vec<ResolvedPath>> {
    let lines: Vec<&str> = stdout.lines().collect();
    ensure!(
        lines.len() == expected,
        "expected {expected} result lines from the path resolution script, got {}: {stdout:?}",
        lines.len()
    );
    lines
        .into_iter()
        .map(|line| {
            let mut parts = line.splitn(3, ' ');
            let (Some(translate), Some(exists), Some(path)) =
                (parts.next(), parts.next(), parts.next())
            else {
                bail!("malformed line from the path resolution script: {line:?}");
            };
            let used_fallback = match translate {
                "ok" => false,
                "fallback" => true,
                _ => bail!("malformed line from the path resolution script: {line:?}"),
            };
            let exists = match exists {
                "ok" => true,
                "missing" => false,
                _ => bail!("malformed line from the path resolution script: {line:?}"),
            };
            ensure!(
                path.starts_with('/'),
                "unexpected resolved path from the path resolution script: {path:?}"
            );
            Ok(ResolvedPath {
                path: path.to_string(),
                used_fallback,
                exists,
            })
        })
        .collect()
}

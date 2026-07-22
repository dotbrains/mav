use super::*;

pub(super) fn is_service_file(path: &str) -> bool {
    let path = path.trim();
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .trim_start_matches("./");

    if path.is_empty() || path == "/dev/null" {
        return true;
    }

    let file_name = path.rsplit('/').next().unwrap_or(path);
    if matches!(
        file_name,
        "package.json"
            | "package-lock.json"
            | "pnpm-lock.yaml"
            | "Cargo.lock"
            | "yarn.lock"
            | "bun.lock"
            | "bun.lockb"
            | "go.sum"
            | "composer.lock"
            | "Gemfile.lock"
            | "Pipfile.lock"
            | "poetry.lock"
            | "uv.lock"
            | ".gitlab-ci.yml"
            | ".travis.yml"
            | "azure-pipelines.yml"
            | "Jenkinsfile"
    ) {
        return true;
    }

    if file_name.ends_with(".min.js")
        || file_name.ends_with(".bundle.js")
        || file_name.contains(".generated.")
        || file_name.ends_with(".pb.go")
    {
        return true;
    }

    if path == ".github/workflows"
        || path.starts_with(".github/workflows/")
        || path == ".circleci"
        || path.starts_with(".circleci/")
    {
        return true;
    }

    path.split('/').any(|component| {
        matches!(
            component,
            "dist" | "build" | "coverage" | "node_modules" | "vendor"
        )
    })
}

pub(super) fn edit_starts_on_service_file(patch: &Patch, split_pos: usize) -> bool {
    locate_edited_line(patch, split_pos as isize)
        .is_some_and(|edit_location| is_service_file(&edit_location.filename))
}

pub(super) fn has_submodule_gitlink_hunk(commit: &str) -> bool {
    commit.lines().any(line_indicates_submodule_gitlink)
}

pub(super) fn line_indicates_submodule_gitlink(line: &str) -> bool {
    let line = line.trim();

    matches!(
        line,
        "new file mode 160000" | "deleted file mode 160000" | "old mode 160000" | "new mode 160000"
    ) || line
        .strip_prefix("index ")
        .and_then(|line| line.split_whitespace().last())
        .is_some_and(|mode| mode == "160000")
        || line
            .strip_prefix('+')
            .or_else(|| line.strip_prefix('-'))
            .is_some_and(|line| line.starts_with("Subproject commit "))
}

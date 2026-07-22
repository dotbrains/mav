use super::*;

mod pyproject_manifest_tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use language::{ManifestDelegate, ManifestProvider, ManifestQuery};
    use settings::WorktreeId;
    use util::rel_path::RelPath;

    use crate::python::PyprojectTomlManifestProvider;

    struct FakeManifestDelegate {
        existing_files: HashSet<&'static str>,
    }

    impl ManifestDelegate for FakeManifestDelegate {
        fn worktree_id(&self) -> WorktreeId {
            WorktreeId::from_usize(0)
        }

        fn exists(&self, path: &RelPath, _is_dir: Option<bool>) -> bool {
            self.existing_files.contains(path.as_unix_str())
        }
    }

    fn search(files: &[&'static str], query_path: &str) -> Option<Arc<RelPath>> {
        let delegate = Arc::new(FakeManifestDelegate {
            existing_files: files.iter().copied().collect(),
        });
        let provider = PyprojectTomlManifestProvider;
        provider.search(ManifestQuery {
            path: RelPath::unix(query_path).unwrap().into(),
            depth: 10,
            delegate,
        })
    }

    #[test]
    fn test_simple_project_no_lockfile() {
        let result = search(&["project/pyproject.toml"], "project/src/main.py");
        assert_eq!(result.as_deref(), RelPath::unix("project").ok());
    }

    #[test]
    fn test_uv_workspace_returns_root() {
        let result = search(
            &[
                "pyproject.toml",
                "uv.lock",
                "packages/subproject/pyproject.toml",
            ],
            "packages/subproject/src/main.py",
        );
        assert_eq!(result.as_deref(), RelPath::unix("").ok());
    }

    #[test]
    fn test_poetry_workspace_returns_root() {
        let result = search(
            &["pyproject.toml", "poetry.lock", "libs/mylib/pyproject.toml"],
            "libs/mylib/src/main.py",
        );
        assert_eq!(result.as_deref(), RelPath::unix("").ok());
    }

    #[test]
    fn test_pdm_workspace_returns_root() {
        let result = search(
            &[
                "pyproject.toml",
                "pdm.lock",
                "packages/mypackage/pyproject.toml",
            ],
            "packages/mypackage/src/main.py",
        );
        assert_eq!(result.as_deref(), RelPath::unix("").ok());
    }

    #[test]
    fn test_independent_subprojects_no_lockfile_at_root() {
        let result_a = search(
            &["project-a/pyproject.toml", "project-b/pyproject.toml"],
            "project-a/src/main.py",
        );
        assert_eq!(result_a.as_deref(), RelPath::unix("project-a").ok());

        let result_b = search(
            &["project-a/pyproject.toml", "project-b/pyproject.toml"],
            "project-b/src/main.py",
        );
        assert_eq!(result_b.as_deref(), RelPath::unix("project-b").ok());
    }

    #[test]
    fn test_no_pyproject_returns_none() {
        let result = search(&[], "src/main.py");
        assert_eq!(result, None);
    }

    #[test]
    fn test_subproject_with_own_lockfile_and_workspace_root() {
        // Both root and subproject have lockfiles; should return root (outermost)
        let result = search(
            &[
                "pyproject.toml",
                "uv.lock",
                "packages/sub/pyproject.toml",
                "packages/sub/uv.lock",
            ],
            "packages/sub/src/main.py",
        );
        assert_eq!(result.as_deref(), RelPath::unix("").ok());
    }

    #[test]
    fn test_depth_limits_search() {
        let delegate = Arc::new(FakeManifestDelegate {
            existing_files: ["pyproject.toml", "uv.lock", "deep/nested/pyproject.toml"]
                .into_iter()
                .collect(),
        });
        let provider = PyprojectTomlManifestProvider;
        // depth=3 from "deep/nested/src/main.py" searches:
        //   "deep/nested/src/main.py", "deep/nested/src", and "deep/nested"
        // It won't reach "deep" or root ""
        let result = provider.search(ManifestQuery {
            path: RelPath::unix("deep/nested/src/main.py").unwrap().into(),
            depth: 3,
            delegate,
        });
        assert_eq!(result.as_deref(), RelPath::unix("deep/nested").ok());
    }
}

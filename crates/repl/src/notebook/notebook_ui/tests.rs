use super::*;
use gpui::TestAppContext;
use project::{FakeFs, Project, ProjectItem as _};
use serde_json::json;
use settings::SettingsStore;
use util::path;
use util::rel_path::rel_path;

const NOTEBOOK_WITH_ONE_CODE_CELL: &str = r#"{
    "metadata": {
        "kernelspec": {
            "display_name": "Python 3",
            "language": "python",
            "name": "python3"
        },
        "language_info": {
            "name": "python"
        }
    },
    "nbformat": 4,
    "nbformat_minor": 5,
    "cells": [
        {
            "cell_type": "code",
            "id": "cell-one",
            "metadata": {},
            "execution_count": null,
            "outputs": [],
            "source": ["print('hello')"]
        }
    ]
}"#;

/// When the configured interpreter doesn't exist (e.g. Python isn't installed),
/// running a cell must not leave it stuck in the executing state. It should
/// instead surface the kernel launch error as an error output on the cell.
#[gpui::test]
async fn test_run_cell_with_missing_interpreter_shows_error(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/notebooks"),
        json!({ "test.ipynb": NOTEBOOK_WITH_ONE_CODE_CELL }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/notebooks").as_ref()], cx).await;
    cx.update(|cx| ReplStore::init(fs.clone(), cx));

    let worktree_id = project.read_with(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    // Select a kernel whose interpreter doesn't exist, simulating a machine
    // where Python isn't installed properly. This is the same path the
    // kernel picker uses.
    let missing_interpreter = path!("/nonexistent/python3");
    let broken_spec = KernelSpecification::Jupyter(LocalKernelSpecification {
        name: "python3".to_string(),
        path: PathBuf::from(missing_interpreter),
        kernelspec: JupyterKernelspec {
            argv: vec![
                missing_interpreter.to_string(),
                "-m".to_string(),
                "ipykernel_launcher".to_string(),
                "-f".to_string(),
                "{connection_file}".to_string(),
            ],
            display_name: "Python 3".to_string(),
            language: "python".to_string(),
            interrupt_mode: None,
            metadata: None,
            env: None,
        },
    });
    cx.update(|cx| {
        ReplStore::global(cx).update(cx, |store, cx| {
            store.set_active_kernelspec(worktree_id, broken_spec, cx);
        })
    });

    let notebook_item = cx
        .update(|cx| {
            NotebookItem::try_open(
                &project,
                &ProjectPath {
                    worktree_id,
                    path: rel_path("test.ipynb").into(),
                },
                cx,
            )
            .expect("ipynb files should be openable as notebooks")
        })
        .await
        .expect("notebook should parse");

    // Don't render the notebook UI itself: its animated kernel status icon
    // schedules a new frame on every render, which makes `run_until_parked`
    // spin forever in tests. The editor entity is created inside an empty
    // window instead; we are testing execution behavior, not rendering.
    let cx = cx.add_empty_window();

    // Launching a kernel probes real TCP ports on localhost, which the
    // deterministic test scheduler cannot drive.
    cx.executor().allow_parking();

    let editor = cx.update(|window, cx| {
        cx.new(|cx| NotebookEditor::new(project.clone(), notebook_item, window, cx))
    });

    // Creating the editor launches the kernel. Wait for the actual launch
    // task, which fails because the interpreter cannot be spawned.
    let pending_kernel = editor.read_with(cx, |editor, _| match &editor.kernel {
        Kernel::StartingKernel(task) => task.clone(),
        _ => panic!("kernel should be starting right after the editor is created"),
    });
    pending_kernel.await;

    editor.read_with(cx, |editor, _| {
        assert!(
            matches!(editor.kernel, Kernel::ErroredLaunch(_)),
            "kernel launch should fail, instead status is: {}",
            editor.kernel.status().to_string()
        );
    });

    // Run the (only) cell via the production action handler.
    editor.update_in(cx, |editor, window, cx| {
        editor.run_current_cell(&Run, window, cx);
    });

    editor.read_with(cx, |editor, cx| {
        let cell_id = editor.cell_order.first().expect("notebook has one cell");
        let Some(Cell::Code(cell)) = editor.cell_map.get(cell_id) else {
            panic!("expected a code cell");
        };
        let cell = cell.read(cx);

        assert!(
            !cell.is_executing(),
            "cell must not be stuck in the executing state when the kernel is not running"
        );

        let nbformat::v4::Cell::Code { outputs, .. } = cell.to_nbformat_cell(cx) else {
            panic!("expected a code cell");
        };
        match outputs.as_slice() {
            [nbformat::v4::Output::Error(error)] => {
                assert_eq!(error.ename, "Kernel Error");
                let traceback = error.traceback.join("\n");
                assert!(
                    traceback.contains("the kernel failed to launch"),
                    "error output should explain why the cell could not run, got: {traceback}"
                );
            }
            other => panic!("expected a single error output, got: {other:?}"),
        }
    });
}

use super::*;

impl NotebookEditor {
    pub(super) fn launch_kernel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let spec = self.kernel_specification.clone().or_else(|| {
            ReplStore::global(cx)
                .read(cx)
                .active_kernelspec(self.worktree_id, None, cx)
        });

        let spec = spec.unwrap_or_else(|| {
            KernelSpecification::Jupyter(LocalKernelSpecification {
                name: "python3".to_string(),
                path: PathBuf::from("python3"),
                kernelspec: JupyterKernelspec {
                    argv: vec![
                        "python3".to_string(),
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
            })
        });

        self.launch_kernel_with_spec(spec, window, cx);
    }

    pub(super) fn launch_kernel_with_spec(
        &mut self,
        spec: KernelSpecification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity_id = cx.entity_id();
        let working_directory = self
            .project
            .read(cx)
            .worktree_for_id(self.worktree_id, cx)
            .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
            .unwrap_or_else(std::env::temp_dir);
        let fs = self.project.read(cx).fs().clone();
        let view = cx.entity();

        self.kernel_specification = Some(spec.clone());

        self.notebook_item.update(cx, |item, cx| {
            let kernel_name = spec.name().to_string();
            let language = spec.language().to_string();

            let display_name = match &spec {
                KernelSpecification::Jupyter(s) => s.kernelspec.display_name.clone(),
                KernelSpecification::PythonEnv(s) => s.kernelspec.display_name.clone(),
                KernelSpecification::JupyterServer(s) => s.kernelspec.display_name.clone(),
                KernelSpecification::SshRemote(s) => s.kernelspec.display_name.clone(),
                KernelSpecification::WslRemote(s) => s.kernelspec.display_name.clone(),
            };

            let kernelspec_json = serde_json::json!({
                "display_name": display_name,
                "name": kernel_name,
                "language": language
            });

            if let Ok(k) = serde_json::from_value(kernelspec_json) {
                item.notebook.metadata.kernelspec = Some(k);
                cx.emit(());
            }
        });

        let kernel_task = match spec {
            KernelSpecification::Jupyter(local_spec) => NativeRunningKernel::new(
                local_spec,
                entity_id,
                working_directory,
                fs,
                view,
                window,
                cx,
            ),
            KernelSpecification::PythonEnv(env_spec) => NativeRunningKernel::new(
                env_spec.as_local_spec(),
                entity_id,
                working_directory,
                fs,
                view,
                window,
                cx,
            ),
            KernelSpecification::JupyterServer(remote_spec) => {
                RemoteRunningKernel::new(remote_spec, working_directory, view, window, cx)
            }

            KernelSpecification::SshRemote(spec) => {
                let project = self.project.clone();
                SshRunningKernel::new(spec, working_directory, project, view, window, cx)
            }
            KernelSpecification::WslRemote(spec) => {
                WslRunningKernel::new(spec, entity_id, working_directory, fs, view, window, cx)
            }
        };

        let pending_kernel = cx
            .spawn(async move |this, cx| {
                let kernel = kernel_task.await;

                match kernel {
                    Ok(kernel) => {
                        this.update(cx, |editor, cx| {
                            editor.kernel = Kernel::RunningKernel(kernel);
                            cx.notify();
                        })
                        .ok();
                    }
                    Err(err) => {
                        log::error!("Kernel failed to start: {:?}", err);
                        this.update(cx, |editor, cx| {
                            editor.kernel = Kernel::ErroredLaunch(err.to_string());
                            cx.notify();
                        })
                        .ok();
                    }
                }
            })
            .shared();

        self.kernel = Kernel::StartingKernel(pending_kernel);
        cx.notify();
    }

    // Note: Python environments are only detected as kernels if ipykernel is installed.
    // Users need to run `pip install ipykernel` (or `uv pip install ipykernel`) in their
    // virtual environment for it to appear in the kernel selector.
    // This happens because we have an ipykernel check inside the function python_env_kernel_specification in mod.rs L:121

    pub(super) fn change_kernel(
        &mut self,
        spec: KernelSpecification,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Kernel::RunningKernel(kernel) = &mut self.kernel {
            kernel.force_shutdown(window, cx).detach();
        }

        self.execution_requests.clear();

        self.launch_kernel_with_spec(spec, window, cx);
    }

    pub(super) fn restart_kernel(
        &mut self,
        _: &RestartKernel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(spec) = self.kernel_specification.clone() {
            if let Kernel::RunningKernel(kernel) = &mut self.kernel {
                kernel.force_shutdown(window, cx).detach();
            }

            self.kernel = Kernel::Restarting;
            cx.notify();

            self.launch_kernel_with_spec(spec, window, cx);
        }
    }

    pub(super) fn interrupt_kernel(
        &mut self,
        _: &InterruptKernel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Kernel::RunningKernel(kernel) = &self.kernel {
            let interrupt_request = runtimelib::InterruptRequest {};
            let message: JupyterMessage = interrupt_request.into();
            kernel.request_tx().try_send(message).ok();
            cx.notify();
        }
    }

    pub(super) fn execute_cell(
        &mut self,
        cell_id: CellId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let code = if let Some(Cell::Code(cell)) = self.cell_map.get(&cell_id) {
            let editor = cell.read(cx).editor().clone();
            let buffer = editor.read(cx).buffer().read(cx);
            buffer
                .as_singleton()
                .map(|b| b.read(cx).text())
                .unwrap_or_default()
        } else {
            return;
        };

        let request = ExecuteRequest {
            code,
            ..Default::default()
        };
        let message: JupyterMessage = request.into();
        let msg_id = message.header.msg_id.clone();

        let send_result = match &mut self.kernel {
            Kernel::RunningKernel(kernel) => kernel
                .request_tx()
                .try_send(message)
                .map_err(|err| format!("failed to send execute request to kernel (the kernel process may have died): {err}")),
            Kernel::StartingKernel(_) => Err("the kernel is still starting".to_string()),
            Kernel::ErroredLaunch(error) => Err(format!("the kernel failed to launch: {error}")),
            Kernel::ShuttingDown | Kernel::Shutdown => Err("the kernel is shut down".to_string()),
            Kernel::Restarting => Err("the kernel is restarting".to_string()),
        };

        if let Some(Cell::Code(cell)) = self.cell_map.get(&cell_id) {
            cell.update(cx, |cell, cx| {
                if cell.has_outputs() {
                    cell.clear_outputs();
                }
                if let Err(error) = &send_result {
                    cell.show_kernel_error(error, window, cx);
                } else {
                    cell.start_execution();
                }
                cx.notify();
            });
        }

        if let Err(error) = send_result {
            log::error!("notebook: cannot execute cell: {error}");
        } else {
            self.execution_requests.insert(msg_id, cell_id.clone());
        }
    }
}

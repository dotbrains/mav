use super::*;

impl Session {
    pub(super) fn fetch<T: LocalDapCommand + PartialEq + Eq + Hash>(
        &mut self,
        request: T,
        process_result: impl FnOnce(&mut Self, Result<T::Response>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        const {
            assert!(
                T::CACHEABLE,
                "Only requests marked as cacheable should invoke `fetch`"
            );
        }

        if (!self.active_snapshot.thread_states.any_stopped_thread()
            && request.type_id() != TypeId::of::<ThreadsCommand>())
            || self.selected_snapshot_index.is_some()
            || self.is_session_terminated
        {
            return;
        }

        let request_map = self
            .requests
            .entry(std::any::TypeId::of::<T>())
            .or_default();

        if let Entry::Vacant(vacant) = request_map.entry(request.into()) {
            let command = vacant.key().0.clone().as_any_arc().downcast::<T>().unwrap();

            let task = Self::request_inner::<Arc<T>>(
                &self.capabilities,
                &self.state,
                command,
                |this, result, cx| {
                    process_result(this, result, cx);
                    None
                },
                cx,
            );
            let task = cx
                .background_executor()
                .spawn(async move {
                    let _ = task.await?;
                    Some(())
                })
                .shared();

            vacant.insert(task);
            cx.notify();
        }
    }

    fn request_inner<T: LocalDapCommand + PartialEq + Eq + Hash>(
        capabilities: &Capabilities,
        mode: &SessionState,
        request: T,
        process_result: impl FnOnce(
            &mut Self,
            Result<T::Response>,
            &mut Context<Self>,
        ) -> Option<T::Response>
        + 'static,
        cx: &mut Context<Self>,
    ) -> Task<Option<T::Response>> {
        if !T::is_supported(capabilities) {
            log::warn!(
                "Attempted to send a DAP request that isn't supported: {:?}",
                request
            );
            let error = Err(anyhow::Error::msg(
                "Couldn't complete request because it's not supported",
            ));
            return cx.spawn(async move |this, cx| {
                this.update(cx, |this, cx| process_result(this, error, cx))
                    .ok()
                    .flatten()
            });
        }

        let request = mode.request_dap(request);
        cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| process_result(this, result, cx))
                .ok()
                .flatten()
        })
    }

    pub(super) fn request<T: LocalDapCommand + PartialEq + Eq + Hash>(
        &self,
        request: T,
        process_result: impl FnOnce(
            &mut Self,
            Result<T::Response>,
            &mut Context<Self>,
        ) -> Option<T::Response>
        + 'static,
        cx: &mut Context<Self>,
    ) -> Task<Option<T::Response>> {
        Self::request_inner(&self.capabilities, &self.state, request, process_result, cx)
    }

    pub(super) fn invalidate_command_type<Command: LocalDapCommand>(&mut self) {
        self.requests.remove(&std::any::TypeId::of::<Command>());
    }

    pub(super) fn invalidate_generic(&mut self) {
        self.invalidate_command_type::<ModulesCommand>();
        self.invalidate_command_type::<LoadedSourcesCommand>();
        self.invalidate_command_type::<ThreadsCommand>();
        self.invalidate_command_type::<DataBreakpointInfoCommand>();
        self.invalidate_command_type::<ReadMemory>();
        let executor = self.as_running().map(|running| running.executor.clone());
        if let Some(executor) = executor {
            self.memory.clear(&executor);
        }
    }

    pub(super) fn invalidate_state(&mut self, key: &RequestSlot) {
        self.requests
            .entry((&*key.0 as &dyn Any).type_id())
            .and_modify(|request_map| {
                request_map.remove(key);
            });
    }
}

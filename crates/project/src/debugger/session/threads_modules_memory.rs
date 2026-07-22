use super::*;

impl Session {
    pub fn any_stopped_thread(&self) -> bool {
        self.active_snapshot.thread_states.any_stopped_thread()
    }

    pub fn thread_status(&self, thread_id: ThreadId) -> ThreadStatus {
        self.active_snapshot.thread_states.thread_status(thread_id)
    }

    pub fn threads(&mut self, cx: &mut Context<Self>) -> Vec<(dap::Thread, ThreadStatus)> {
        self.fetch(
            dap_command::ThreadsCommand,
            |this, result, cx| {
                let Some(result) = result.log_err() else {
                    return;
                };

                this.active_snapshot.threads = result
                    .into_iter()
                    .map(|thread| (ThreadId(thread.id), Thread::from(thread)))
                    .collect();

                this.invalidate_command_type::<StackTraceCommand>();
                cx.emit(SessionEvent::Threads);
                cx.notify();
            },
            cx,
        );

        let state = self.session_state();
        state
            .threads
            .values()
            .map(|thread| {
                (
                    thread.dap.clone(),
                    state.thread_states.thread_status(ThreadId(thread.dap.id)),
                )
            })
            .collect()
    }

    pub fn modules(&mut self, cx: &mut Context<Self>) -> &[Module] {
        self.fetch(
            dap_command::ModulesCommand,
            |this, result, cx| {
                let Some(result) = result.log_err() else {
                    return;
                };

                this.active_snapshot.modules = result;
                cx.emit(SessionEvent::Modules);
                cx.notify();
            },
            cx,
        );

        &self.session_state().modules
    }

    // CodeLLDB returns the size of a pointed-to-memory, which we can use to make the experience of go-to-memory better.
    pub fn data_access_size(
        &mut self,
        frame_id: Option<u64>,
        evaluate_name: &str,
        cx: &mut Context<Self>,
    ) -> Task<Option<u64>> {
        let request = self.request(
            EvaluateCommand {
                expression: format!("?${{sizeof({evaluate_name})}}"),
                frame_id,

                context: Some(EvaluateArgumentsContext::Repl),
                source: None,
            },
            |_, response, _| response.ok(),
            cx,
        );
        cx.background_spawn(async move {
            let result = request.await?;
            result.result.parse().ok()
        })
    }

    pub fn memory_reference_of_expr(
        &mut self,
        frame_id: Option<u64>,
        expression: String,
        cx: &mut Context<Self>,
    ) -> Task<Option<(String, Option<String>)>> {
        let request = self.request(
            EvaluateCommand {
                expression,
                frame_id,

                context: Some(EvaluateArgumentsContext::Repl),
                source: None,
            },
            |_, response, _| response.ok(),
            cx,
        );
        cx.background_spawn(async move {
            let result = request.await?;
            result
                .memory_reference
                .map(|reference| (reference, result.type_))
        })
    }

    pub fn write_memory(&mut self, address: u64, data: &[u8], cx: &mut Context<Self>) {
        let data = base64::engine::general_purpose::STANDARD.encode(data);
        self.request(
            WriteMemoryArguments {
                memory_reference: address.to_string(),
                data,
                allow_partial: None,
                offset: None,
            },
            |this, response, cx| {
                this.memory.clear(cx.background_executor());
                this.invalidate_command_type::<ReadMemory>();
                this.invalidate_command_type::<VariablesCommand>();
                cx.emit(SessionEvent::Variables);
                response.ok()
            },
            cx,
        )
        .detach();
    }
    pub fn read_memory(
        &mut self,
        range: RangeInclusive<u64>,
        cx: &mut Context<Self>,
    ) -> MemoryIterator {
        // This function is a bit more involved when it comes to fetching data.
        // Since we attempt to read memory in pages, we need to account for some parts
        // of memory being unreadable. Therefore, we start off by fetching a page per request.
        // In case that fails, we try to re-fetch smaller regions until we have the full range.
        let page_range = Memory::memory_range_to_page_range(range.clone());
        for page_address in PageAddress::iter_range(page_range) {
            self.read_single_page_memory(page_address, cx);
        }
        self.memory.memory_range(range)
    }

    fn read_single_page_memory(&mut self, page_start: PageAddress, cx: &mut Context<Self>) {
        _ = maybe!({
            let builder = self.memory.build_page(page_start)?;

            self.memory_read_fetch_page_recursive(builder, cx);
            Some(())
        });
    }
    fn memory_read_fetch_page_recursive(
        &mut self,
        mut builder: MemoryPageBuilder,
        cx: &mut Context<Self>,
    ) {
        let Some(next_request) = builder.next_request() else {
            // We're done fetching. Let's grab the page and insert it into our memory store.
            let (address, contents) = builder.build();
            self.memory.insert_page(address, contents);

            return;
        };
        let size = next_request.size;
        self.fetch(
            ReadMemory {
                memory_reference: format!("0x{:X}", next_request.address),
                offset: Some(0),
                count: next_request.size,
            },
            move |this, memory, cx| {
                if let Ok(memory) = memory {
                    builder.known(memory.content);
                    if let Some(unknown) = memory.unreadable_bytes {
                        builder.unknown(unknown);
                    }
                    // This is the recursive bit: if we're not yet done with
                    // the whole page, we'll kick off a new request with smaller range.
                    // Note that this function is recursive only conceptually;
                    // since it kicks off a new request with callback, we don't need to worry about stack overflow.
                    this.memory_read_fetch_page_recursive(builder, cx);
                } else {
                    builder.unknown(size);
                }
            },
            cx,
        );
    }
}

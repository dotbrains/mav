use super::*;

impl AcpThread {
    pub fn push_user_content_block(
        &mut self,
        client_id: Option<ClientUserMessageId>,
        chunk: acp::ContentBlock,
        cx: &mut Context<Self>,
    ) {
        self.push_user_content_block_with_indent(client_id, chunk, false, cx)
    }

    pub fn push_user_content_block_with_indent(
        &mut self,
        client_id: Option<ClientUserMessageId>,
        chunk: acp::ContentBlock,
        indented: bool,
        cx: &mut Context<Self>,
    ) {
        self.push_user_content_block_with_protocol_id(
            client_id.clone(),
            client_id.is_some(),
            None,
            chunk,
            indented,
            cx,
        )
    }

    pub(super) fn push_user_content_block_from_agent(
        &mut self,
        id: Option<acp::MessageId>,
        chunk: acp::ContentBlock,
        cx: &mut Context<Self>,
    ) {
        self.push_user_content_block_with_protocol_id(None, false, id, chunk, false, cx)
    }

    pub(super) fn push_user_content_block_with_protocol_id(
        &mut self,
        incoming_client_id: Option<ClientUserMessageId>,
        is_optimistic: bool,
        protocol_id: Option<acp::MessageId>,
        chunk: acp::ContentBlock,
        indented: bool,
        cx: &mut Context<Self>,
    ) {
        let language_registry = self.project.read(cx).languages().clone();
        let path_style = self.project.read(cx).path_style(cx);
        let entries_len = self.entries.len();

        if let Some(last_entry) = self.entries.last_mut()
            && let AgentThreadEntry::UserMessage(UserMessage {
                protocol_id: existing_protocol_id,
                client_id: existing_client_id,
                content,
                chunks,
                is_optimistic: existing_is_optimistic,
                indented: existing_indented,
                ..
            }) = last_entry
            && *existing_indented == indented
            && can_merge_message_chunks(existing_protocol_id.as_ref(), protocol_id.as_ref())
            && !(*existing_is_optimistic
                && !is_optimistic
                && existing_protocol_id.is_none()
                && protocol_id.is_some())
        {
            Self::flush_streaming_text(&mut self.streaming_text_buffer, cx);
            if let Some(incoming_client_id) = incoming_client_id {
                *existing_client_id = Some(incoming_client_id);
            }
            *existing_is_optimistic |= is_optimistic;
            if existing_protocol_id.is_none() {
                *existing_protocol_id = protocol_id;
            }
            content.append(chunk.clone(), &language_registry, path_style, cx);
            chunks.push(chunk);
            let idx = entries_len - 1;
            cx.emit(AcpThreadEvent::EntryUpdated(idx));
        } else {
            let content = ContentBlock::new(chunk.clone(), &language_registry, path_style, cx);
            self.push_entry(
                AgentThreadEntry::UserMessage(UserMessage {
                    protocol_id,
                    client_id: incoming_client_id,
                    is_optimistic,
                    content,
                    chunks: vec![chunk],
                    checkpoint: None,
                    indented,
                }),
                cx,
            );
        }
    }

    pub fn push_assistant_content_block(
        &mut self,
        chunk: acp::ContentBlock,
        is_thought: bool,
        cx: &mut Context<Self>,
    ) {
        self.push_assistant_content_block_with_indent(chunk, is_thought, false, cx)
    }

    pub fn push_assistant_content_block_with_indent(
        &mut self,
        chunk: acp::ContentBlock,
        is_thought: bool,
        indented: bool,
        cx: &mut Context<Self>,
    ) {
        self.push_assistant_content_block_with_message_id(None, chunk, is_thought, indented, cx)
    }

    pub(super) fn push_assistant_content_block_with_message_id(
        &mut self,
        message_id: Option<acp::MessageId>,
        chunk: acp::ContentBlock,
        is_thought: bool,
        indented: bool,
        cx: &mut Context<Self>,
    ) {
        let path_style = self.project.read(cx).path_style(cx);

        // For text chunks going to an existing Markdown block, buffer for smooth
        // streaming instead of appending all at once which may feel more choppy.
        if let acp::ContentBlock::Text(text_content) = &chunk {
            if let Some(markdown) =
                self.streaming_markdown_target(message_id.as_ref(), is_thought, indented)
            {
                let entries_len = self.entries.len();
                cx.emit(AcpThreadEvent::EntryUpdated(entries_len - 1));
                self.buffer_streaming_text(&markdown, text_content.text.clone(), cx);
                return;
            }
        }

        let language_registry = self.project.read(cx).languages().clone();
        let entries_len = self.entries.len();
        if let Some(last_entry) = self.entries.last_mut()
            && let AgentThreadEntry::AssistantMessage(AssistantMessage {
                chunks,
                indented: existing_indented,
                is_subagent_output: _,
            }) = last_entry
            && *existing_indented == indented
        {
            let idx = entries_len - 1;
            Self::flush_streaming_text(&mut self.streaming_text_buffer, cx);
            cx.emit(AcpThreadEvent::EntryUpdated(idx));
            match (chunks.last_mut(), is_thought) {
                (
                    Some(AssistantMessageChunk::Message {
                        id: existing_id,
                        block,
                    }),
                    false,
                )
                | (
                    Some(AssistantMessageChunk::Thought {
                        id: existing_id,
                        block,
                    }),
                    true,
                ) if can_merge_message_chunks(existing_id.as_ref(), message_id.as_ref()) => {
                    if existing_id.is_none() {
                        *existing_id = message_id;
                    }
                    block.append(chunk, &language_registry, path_style, cx)
                }
                _ => {
                    let block = ContentBlock::new(chunk, &language_registry, path_style, cx);
                    if is_thought {
                        chunks.push(AssistantMessageChunk::Thought {
                            id: message_id,
                            block,
                        })
                    } else {
                        chunks.push(AssistantMessageChunk::Message {
                            id: message_id,
                            block,
                        })
                    }
                }
            }
        } else {
            let block = ContentBlock::new(chunk, &language_registry, path_style, cx);
            let chunk = if is_thought {
                AssistantMessageChunk::Thought {
                    id: message_id,
                    block,
                }
            } else {
                AssistantMessageChunk::Message {
                    id: message_id,
                    block,
                }
            };

            self.push_entry(
                AgentThreadEntry::AssistantMessage(AssistantMessage {
                    chunks: vec![chunk],
                    indented,
                    is_subagent_output: false,
                }),
                cx,
            );
        }
    }

    pub(super) fn streaming_markdown_target(
        &mut self,
        message_id: Option<&acp::MessageId>,
        is_thought: bool,
        indented: bool,
    ) -> Option<Entity<Markdown>> {
        let last_entry = self.entries.last_mut()?;
        if let AgentThreadEntry::AssistantMessage(AssistantMessage {
            chunks,
            indented: existing_indented,
            ..
        }) = last_entry
            && *existing_indented == indented
            && let [.., chunk] = chunks.as_mut_slice()
        {
            match (chunk, is_thought) {
                (
                    AssistantMessageChunk::Message {
                        id: existing_id,
                        block: ContentBlock::Markdown { markdown },
                    },
                    false,
                )
                | (
                    AssistantMessageChunk::Thought {
                        id: existing_id,
                        block: ContentBlock::Markdown { markdown },
                    },
                    true,
                ) if can_merge_message_chunks(existing_id.as_ref(), message_id) => {
                    if existing_id.is_none() {
                        *existing_id = message_id.cloned();
                    }
                    Some(markdown.clone())
                }
                _ => None,
            }
        } else {
            None
        }
    }

    /// Add text to the streaming buffer. If the target changed (e.g. switching
    /// from thoughts to message text), flush the old buffer first.
    pub(super) fn buffer_streaming_text(
        &mut self,
        markdown: &Entity<Markdown>,
        text: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(buffer) = &mut self.streaming_text_buffer {
            if buffer.target.entity_id() == markdown.entity_id() {
                buffer.pending.push_str(&text);

                buffer.bytes_to_reveal_per_tick = (buffer.pending.len() as f32
                    / StreamingTextBuffer::REVEAL_TARGET
                    * StreamingTextBuffer::TASK_UPDATE_MS as f32)
                    .ceil() as usize;
                return;
            }
            Self::flush_streaming_text(&mut self.streaming_text_buffer, cx);
        }

        let target = markdown.clone();
        let _reveal_task = self.start_streaming_reveal(cx);
        let pending_len = text.len();
        let bytes_to_reveal = (pending_len as f32 / StreamingTextBuffer::REVEAL_TARGET
            * StreamingTextBuffer::TASK_UPDATE_MS as f32)
            .ceil() as usize;
        self.streaming_text_buffer = Some(StreamingTextBuffer {
            pending: text,
            bytes_to_reveal_per_tick: bytes_to_reveal,
            target,
            _reveal_task,
        });
    }

    /// Flush all buffered streaming text into the Markdown entity immediately.
    pub(super) fn flush_streaming_text(
        streaming_text_buffer: &mut Option<StreamingTextBuffer>,
        cx: &mut Context<Self>,
    ) {
        if let Some(buffer) = streaming_text_buffer.take() {
            if !buffer.pending.is_empty() {
                buffer
                    .target
                    .update(cx, |markdown, cx| markdown.append(&buffer.pending, cx));
            }
        }
    }

    /// Spawns a foreground task that periodically drains
    /// `streaming_text_buffer.pending` into the target `Markdown` entity,
    /// producing smooth, continuous text output.
    pub(super) fn start_streaming_reveal(&self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(StreamingTextBuffer::TASK_UPDATE_MS))
                    .await;

                let should_continue = this
                    .update(cx, |this, cx| {
                        let Some(buffer) = &mut this.streaming_text_buffer else {
                            return false;
                        };

                        if buffer.pending.is_empty() {
                            return true;
                        }

                        let pending_len = buffer.pending.len();

                        let byte_boundary = buffer
                            .pending
                            .ceil_char_boundary(buffer.bytes_to_reveal_per_tick)
                            .min(pending_len);

                        buffer.target.update(cx, |markdown: &mut Markdown, cx| {
                            markdown.append(&buffer.pending[..byte_boundary], cx);
                            buffer.pending.drain(..byte_boundary);
                        });

                        true
                    })
                    .unwrap_or(false);

                if !should_continue {
                    break;
                }
            }
        })
    }

    pub(super) fn push_entry(&mut self, entry: AgentThreadEntry, cx: &mut Context<Self>) {
        Self::flush_streaming_text(&mut self.streaming_text_buffer, cx);
        self.entries.push(entry);
        cx.emit(AcpThreadEvent::NewEntry);
    }
}

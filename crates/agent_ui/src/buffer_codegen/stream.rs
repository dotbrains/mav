use super::*;

impl CodegenAlternative {
    pub(super) fn handle_stream(
        &mut self,
        model: Arc<dyn LanguageModel>,
        strip_invalid_spans: bool,
        stream: impl 'static + Future<Output = Result<LanguageModelTextStream>>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let anthropic_reporter = AnthropicEventReporter::new(&model, cx);
        let session_id = self.session_id;
        let model_telemetry_id = model.telemetry_id();
        let model_provider_id = model.provider_id().to_string();
        let start_time = Instant::now();

        // Make a new snapshot and re-resolve anchor in case the document was modified.
        // This can happen often if the editor loses focus and is saved + reformatted,
        // as in https://github.com/mav-industries/mav/issues/39088
        self.snapshot = self.buffer.read(cx).snapshot(cx);
        self.range = self.snapshot.anchor_after(self.range.start)
            ..self.snapshot.anchor_after(self.range.end);

        let snapshot = self.snapshot.clone();
        let selected_text = snapshot
            .text_for_range(self.range.start..self.range.end)
            .collect::<Rope>();

        self.selected_text = Some(selected_text.to_string());

        let selection_start = self.range.start.to_point(&snapshot);

        // Start with the indentation of the first line in the selection
        let mut suggested_line_indent = snapshot
            .suggested_indents(selection_start.row..=selection_start.row, cx)
            .into_values()
            .next()
            .unwrap_or_else(|| snapshot.indent_size_for_line(MultiBufferRow(selection_start.row)));

        // If the first line in the selection does not have indentation, check the following lines
        if suggested_line_indent.len == 0 && suggested_line_indent.kind == IndentKind::Space {
            for row in selection_start.row..=self.range.end.to_point(&snapshot).row {
                let line_indent = snapshot.indent_size_for_line(MultiBufferRow(row));
                // Prefer tabs if a line in the selection uses tabs as indentation
                if line_indent.kind == IndentKind::Tab {
                    suggested_line_indent.kind = IndentKind::Tab;
                    break;
                }
            }
        }

        let language_name = {
            let multibuffer = self.buffer.read(cx);
            let snapshot = multibuffer.snapshot(cx);
            let ranges = snapshot.range_to_buffer_ranges(self.range.start..self.range.end);
            ranges
                .first()
                .and_then(|(buffer, _, _)| buffer.language())
                .map(|language| language.name())
        };

        self.diff = Diff::default();
        self.status = CodegenStatus::Pending;
        let mut edit_start = self.range.start.to_offset(&snapshot);
        let completion = Arc::new(Mutex::new(String::new()));
        let completion_clone = completion.clone();

        cx.notify();
        cx.spawn(async move |codegen, cx| {
            let stream = stream.await;

            let token_usage = stream
                .as_ref()
                .ok()
                .map(|stream| stream.last_token_usage.clone());
            let message_id = stream
                .as_ref()
                .ok()
                .and_then(|stream| stream.message_id.clone());
            let generate = async {
                let model_telemetry_id = model_telemetry_id.clone();
                let model_provider_id = model_provider_id.clone();
                let (mut diff_tx, mut diff_rx) = mpsc::channel(1);
                let message_id = message_id.clone();
                let line_based_stream_diff: Task<anyhow::Result<()>> = cx.background_spawn({
                    let anthropic_reporter = anthropic_reporter.clone();
                    let language_name = language_name.clone();
                    async move {
                        let mut response_latency = None;
                        let request_start = Instant::now();
                        let diff = async {
                            let raw_stream = stream?.stream.map_err(|error| error.into());

                            let stripped;
                            let mut chunks: Pin<Box<dyn Stream<Item = Result<String>> + Send>> =
                                if strip_invalid_spans {
                                    stripped = StripInvalidSpans::new(raw_stream);
                                    Box::pin(stripped)
                                } else {
                                    Box::pin(raw_stream)
                                };

                            let mut diff = StreamingDiff::new(selected_text.to_string());
                            let mut line_diff = LineDiff::default();

                            let mut new_text = String::new();
                            let mut base_indent = None;
                            let mut line_indent = None;
                            let mut first_line = true;

                            while let Some(chunk) = chunks.next().await {
                                if response_latency.is_none() {
                                    response_latency = Some(request_start.elapsed());
                                }
                                let chunk = chunk?;
                                completion_clone.lock().push_str(&chunk);

                                let mut lines = chunk.split('\n').peekable();
                                while let Some(line) = lines.next() {
                                    new_text.push_str(line);
                                    if line_indent.is_none()
                                        && let Some(non_whitespace_ch_ix) =
                                            new_text.find(|ch: char| !ch.is_whitespace())
                                    {
                                        line_indent = Some(non_whitespace_ch_ix);
                                        base_indent = base_indent.or(line_indent);

                                        let line_indent = line_indent.unwrap();
                                        let base_indent = base_indent.unwrap();
                                        let indent_delta = line_indent as i32 - base_indent as i32;
                                        let mut corrected_indent_len = cmp::max(
                                            0,
                                            suggested_line_indent.len as i32 + indent_delta,
                                        )
                                            as usize;
                                        if first_line {
                                            corrected_indent_len = corrected_indent_len
                                                .saturating_sub(selection_start.column as usize);
                                        }

                                        let indent_char = suggested_line_indent.char();
                                        let mut indent_buffer = [0; 4];
                                        let indent_str =
                                            indent_char.encode_utf8(&mut indent_buffer);
                                        new_text.replace_range(
                                            ..line_indent,
                                            &indent_str.repeat(corrected_indent_len),
                                        );
                                    }

                                    if line_indent.is_some() {
                                        let char_ops = diff.push_new(&new_text);
                                        line_diff.push_char_operations(&char_ops, &selected_text);
                                        diff_tx
                                            .send((char_ops, line_diff.line_operations()))
                                            .await?;
                                        new_text.clear();
                                    }

                                    if lines.peek().is_some() {
                                        let char_ops = diff.push_new("\n");
                                        line_diff.push_char_operations(&char_ops, &selected_text);
                                        diff_tx
                                            .send((char_ops, line_diff.line_operations()))
                                            .await?;
                                        if line_indent.is_none() {
                                            // Don't write out the leading indentation in empty lines on the next line
                                            // This is the case where the above if statement didn't clear the buffer
                                            new_text.clear();
                                        }
                                        line_indent = None;
                                        first_line = false;
                                    }
                                }
                            }

                            let mut char_ops = diff.push_new(&new_text);
                            char_ops.extend(diff.finish());
                            line_diff.push_char_operations(&char_ops, &selected_text);
                            line_diff.finish(&selected_text);
                            diff_tx
                                .send((char_ops, line_diff.line_operations()))
                                .await?;

                            anyhow::Ok(())
                        };

                        let result = diff.await;

                        let error_message = result.as_ref().err().map(|error| error.to_string());
                        telemetry::event!(
                            "Assistant Responded",
                            kind = "inline",
                            phase = "response",
                            session_id = session_id.to_string(),
                            model = model_telemetry_id,
                            model_provider = model_provider_id,
                            language_name = language_name.as_ref().map(|n| n.to_string()),
                            message_id = message_id.as_deref(),
                            response_latency = response_latency,
                            error_message = error_message.as_deref(),
                        );

                        anthropic_reporter.report(AnthropicEventData {
                            completion_type: AnthropicCompletionType::Editor,
                            event: AnthropicEventType::Response,
                            language_name: language_name.map(|n| n.to_string()),
                            message_id,
                        });

                        result?;
                        Ok(())
                    }
                });

                while let Some((char_ops, line_ops)) = diff_rx.next().await {
                    codegen.update(cx, |codegen, cx| {
                        codegen.last_equal_ranges.clear();

                        let edits = char_ops
                            .into_iter()
                            .filter_map(|operation| match operation {
                                CharOperation::Insert { text } => {
                                    let edit_start = snapshot.anchor_after(edit_start);
                                    Some((edit_start..edit_start, text))
                                }
                                CharOperation::Delete { bytes } => {
                                    let edit_end = edit_start + bytes;
                                    let edit_range = snapshot.anchor_after(edit_start)
                                        ..snapshot.anchor_before(edit_end);
                                    edit_start = edit_end;
                                    Some((edit_range, String::new()))
                                }
                                CharOperation::Keep { bytes } => {
                                    let edit_end = edit_start + bytes;
                                    let edit_range = snapshot.anchor_after(edit_start)
                                        ..snapshot.anchor_before(edit_end);
                                    edit_start = edit_end;
                                    codegen.last_equal_ranges.push(edit_range);
                                    None
                                }
                            })
                            .collect::<Vec<_>>();

                        if codegen.active {
                            codegen.apply_edits(edits.iter().cloned(), cx);
                            codegen.reapply_line_based_diff(line_ops.iter().cloned(), cx);
                        }
                        codegen.edits.extend(edits);
                        codegen.line_operations = line_ops;
                        codegen.edit_position = Some(snapshot.anchor_after(edit_start));

                        cx.notify();
                    })?;
                }

                // Streaming stopped and we have the new text in the buffer, and a line-based diff applied for the whole new buffer.
                // That diff is not what a regular diff is and might look unexpected, ergo apply a regular diff.
                // It's fine to apply even if the rest of the line diffing fails, as no more hunks are coming through `diff_rx`.
                let batch_diff_task =
                    codegen.update(cx, |codegen, cx| codegen.reapply_batch_diff(cx))?;
                let (line_based_stream_diff, ()) = join!(line_based_stream_diff, batch_diff_task);
                line_based_stream_diff?;

                anyhow::Ok(())
            };

            let result = generate.await;
            let elapsed_time = start_time.elapsed().as_secs_f64();

            codegen
                .update(cx, |this, cx| {
                    this.message_id = message_id;
                    this.last_equal_ranges.clear();
                    if let Err(error) = result {
                        this.status = CodegenStatus::Error(error);
                    } else {
                        this.status = CodegenStatus::Done;
                    }
                    this.elapsed_time = Some(elapsed_time);
                    this.completion = Some(completion.lock().clone());
                    if let Some(usage) = token_usage {
                        let usage = usage.lock();
                        telemetry::event!(
                            "Inline Assistant Completion",
                            model = model_telemetry_id,
                            model_provider = model_provider_id,
                            input_tokens = usage.input_tokens,
                            output_tokens = usage.output_tokens,
                        )
                    }

                    cx.emit(CodegenEvent::Finished);
                    cx.notify();
                })
                .ok();
        })
    }
}

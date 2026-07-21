use super::*;

impl AcpThread {
    pub fn read_text_file(
        &self,
        path: PathBuf,
        line: Option<u32>,
        limit: Option<u32>,
        reuse_shared_snapshot: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<String, acp::Error>> {
        // Args are 1-based, move to 0-based
        let line = line.unwrap_or_default().saturating_sub(1);
        let limit = limit.unwrap_or(u32::MAX);
        let project = self.project.clone();
        let action_log = self.action_log.clone();
        let should_update_agent_location = self.parent_session_id.is_none();
        cx.spawn(async move |this, cx| {
            let load = project.update(cx, |project, cx| {
                let path = project
                    .project_path_for_absolute_path(&path, cx)
                    .ok_or_else(|| {
                        acp::Error::resource_not_found(Some(path.display().to_string()))
                    })?;
                Ok::<_, acp::Error>(project.open_buffer(path, cx))
            })?;

            let buffer = load.await?;

            let snapshot = if reuse_shared_snapshot {
                this.read_with(cx, |this, _| {
                    this.shared_buffers.get(&buffer.clone()).cloned()
                })
                .log_err()
                .flatten()
            } else {
                None
            };

            let snapshot = if let Some(snapshot) = snapshot {
                snapshot
            } else {
                action_log.update(cx, |action_log, cx| {
                    action_log.buffer_read(buffer.clone(), cx);
                });

                let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
                this.update(cx, |this, _| {
                    this.shared_buffers.insert(buffer.clone(), snapshot.clone());
                })?;
                snapshot
            };

            let max_point = snapshot.max_point();
            let start_position = Point::new(line, 0);

            if start_position > max_point {
                return Err(acp::Error::invalid_params().data(format!(
                    "Attempting to read beyond the end of the file, line {}:{}",
                    max_point.row + 1,
                    max_point.column
                )));
            }

            let start = snapshot.anchor_before(start_position);
            let end = snapshot.anchor_before(Point::new(line.saturating_add(limit), 0));

            if should_update_agent_location {
                project.update(cx, |project, cx| {
                    project.set_agent_location(
                        Some(AgentLocation {
                            buffer: buffer.downgrade(),
                            position: start,
                        }),
                        cx,
                    );
                });
            }

            Ok(snapshot.text_for_range(start..end).collect::<String>())
        })
    }

    pub fn write_text_file(
        &self,
        path: PathBuf,
        content: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let project = self.project.clone();
        let action_log = self.action_log.clone();
        let should_update_agent_location = self.parent_session_id.is_none();
        cx.spawn(async move |this, cx| {
            let load = project.update(cx, |project, cx| {
                let path = project
                    .project_path_for_absolute_path(&path, cx)
                    .context("invalid path")?;
                anyhow::Ok(project.open_buffer(path, cx))
            });
            let buffer = load?.await?;
            let snapshot = this.update(cx, |this, cx| {
                this.shared_buffers
                    .get(&buffer)
                    .cloned()
                    .unwrap_or_else(|| buffer.read(cx).snapshot())
            })?;
            let edits = cx
                .background_executor()
                .spawn(async move {
                    let old_text = snapshot.text();
                    text_diff(old_text.as_str(), &content)
                        .into_iter()
                        .map(|(range, replacement)| {
                            (snapshot.anchor_range_inside(range), replacement)
                        })
                        .collect::<Vec<_>>()
                })
                .await;

            if should_update_agent_location {
                project.update(cx, |project, cx| {
                    project.set_agent_location(
                        Some(AgentLocation {
                            buffer: buffer.downgrade(),
                            position: edits
                                .last()
                                .map(|(range, _)| range.end)
                                .unwrap_or(Anchor::min_for_buffer(buffer.read(cx).remote_id())),
                        }),
                        cx,
                    );
                });
            }

            let format_on_save = cx.update(|cx| {
                action_log.update(cx, |action_log, cx| {
                    action_log.buffer_read(buffer.clone(), cx);
                });

                let format_on_save = buffer.update(cx, |buffer, cx| {
                    buffer.start_transaction();
                    buffer.edit(edits, None, cx);
                    buffer.end_transaction_with_source(BufferEditSource::Agent, cx);

                    let settings =
                        language::language_settings::LanguageSettings::for_buffer(buffer, cx);

                    settings.format_on_save != FormatOnSave::Off
                });
                action_log.update(cx, |action_log, cx| {
                    action_log.buffer_edited(buffer.clone(), cx);
                });
                format_on_save
            });

            if format_on_save {
                let format_task = project.update(cx, |project, cx| {
                    project.format(
                        HashSet::from_iter([buffer.clone()]),
                        LspFormatTarget::Buffers,
                        false,
                        FormatTrigger::Save,
                        cx,
                    )
                });
                format_task.await.log_err();

                action_log.update(cx, |action_log, cx| {
                    action_log.buffer_edited(buffer.clone(), cx);
                });
            }

            project
                .update(cx, |project, cx| project.save_buffer(buffer, cx))
                .await
        })
    }
}

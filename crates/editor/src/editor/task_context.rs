use super::*;

impl Editor {
    pub(super) fn build_tasks_context(
        project: &Entity<Project>,
        buffer: &Entity<Buffer>,
        buffer_row: u32,
        tasks: &Arc<RunnableTasks>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<task::TaskContext>>> {
        let position = Point::new(buffer_row, tasks.column);
        let range_start = buffer.read(cx).anchor_at(position, Bias::Right);
        let location = Location {
            buffer: buffer.clone(),
            range: range_start..range_start,
        };

        let mut captured_task_variables = TaskVariables::default();
        for (capture_name, value) in tasks.extra_variables.clone() {
            captured_task_variables.insert(
                task::VariableName::Custom(capture_name.into()),
                value.clone(),
            );
        }

        project.update(cx, |project, cx| {
            project.task_store().update(cx, |task_store, cx| {
                task_store.task_context_for_location(captured_task_variables, location, cx)
            })
        })
    }
}

use super::*;

impl Repository {
    pub fn send_job<F, Fut, R>(
        &mut self,
        description: &'static str,
        status: Option<SharedString>,
        job: F,
    ) -> oneshot::Receiver<R>
    where
        F: FnOnce(RepositoryState, AsyncApp) -> Fut + 'static,
        Fut: Future<Output = R> + 'static,
        R: Send + 'static,
    {
        self.send_keyed_job(description, None, status, job)
    }

    pub(super) fn send_keyed_job<F, Fut, R>(
        &mut self,
        description: &'static str,
        key: Option<GitJobKey>,
        status: Option<SharedString>,
        job: F,
    ) -> oneshot::Receiver<R>
    where
        F: FnOnce(RepositoryState, AsyncApp) -> Fut + 'static,
        Fut: Future<Output = R> + 'static,
        R: Send + 'static,
    {
        let (result_tx, result_rx) = futures::channel::oneshot::channel();
        let job_id = post_inc(&mut self.job_id);
        let this = self.this.clone();

        let key_label = key.as_ref().map(super::repository_helpers::format_job_key);
        self.job_debug_queue.add(job_id, description, key_label);

        self.job_sender
            .unbounded_send(GitJob {
                id: job_id,
                key,
                job: Box::new(move |state, cx: &mut AsyncApp| {
                    let job = job(state, cx.clone());
                    cx.spawn(async move |cx| {
                        this.update(cx, |this, cx| {
                            this.job_debug_queue.mark_running(job_id);
                            if let Some(s) = status {
                                this.active_jobs.insert(
                                    job_id,
                                    JobInfo {
                                        start: Instant::now(),
                                        message: s,
                                    },
                                );
                            }
                            cx.notify();
                        })
                        .ok();

                        let result = job.await;

                        this.update(cx, |this, cx| {
                            this.job_debug_queue.mark_complete(
                                job_id,
                                job_debug_queue::CompletedJobStatus::Finished,
                            );
                            this.active_jobs.remove(&job_id);
                            cx.notify();
                        })
                        .ok();

                        result_tx.send(result).ok();
                    })
                }),
            })
            .ok();
        result_rx
    }
}

use super::*;

pub type JobId = usize;

#[derive(Clone, Debug)]
pub struct JobInfo {
    pub start: Instant,
    pub message: SharedString,
    pub id: JobId,
}

#[derive(Debug, Clone)]
pub enum JobEvent {
    Started { info: JobInfo },
    Completed { id: JobId },
}

pub type JobEventSender = futures::channel::mpsc::UnboundedSender<JobEvent>;
pub type JobEventReceiver = futures::channel::mpsc::UnboundedReceiver<JobEvent>;

pub(super) struct JobTracker {
    id: JobId,
    subscribers: Arc<Mutex<Vec<JobEventSender>>>,
}

impl JobTracker {
    pub(super) fn new(info: JobInfo, subscribers: Arc<Mutex<Vec<JobEventSender>>>) -> Self {
        let id = info.id;
        {
            let mut subs = subscribers.lock();
            subs.retain(|sender| {
                sender
                    .unbounded_send(JobEvent::Started { info: info.clone() })
                    .is_ok()
            });
        }
        Self { id, subscribers }
    }
}

impl Drop for JobTracker {
    fn drop(&mut self) {
        let mut subs = self.subscribers.lock();
        subs.retain(|sender| {
            sender
                .unbounded_send(JobEvent::Completed { id: self.id })
                .is_ok()
        });
    }
}

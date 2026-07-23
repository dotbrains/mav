use super::*;

/// constant source, only works on a single span
#[derive(Debug)]
pub(super) struct ReplayQueue {
    inner: ArrayQueue<Vec<Sample>>,
    normal_chunk_len: usize,
    /// The last chunk in the queue may be smaller than
    /// the normal chunk size. This is always equal to the
    /// size of the last element in the queue.
    /// (so normally chunk_size)
    last_chunk: Mutex<Vec<Sample>>,
}

impl ReplayQueue {
    pub(super) fn new(queue_len: usize, chunk_size: usize) -> Self {
        Self {
            inner: ArrayQueue::new(queue_len),
            normal_chunk_len: chunk_size,
            last_chunk: Mutex::new(Vec::new()),
        }
    }
    /// Returns the length in samples
    fn len(&self) -> usize {
        self.inner.len().saturating_sub(1) * self.normal_chunk_len
            + self
                .last_chunk
                .lock()
                .expect("Self::push_last can not poison this lock")
                .len()
    }

    fn pop(&self) -> Option<Vec<Sample>> {
        self.inner.pop() // removes element that was inserted first
    }

    fn push_last(&self, mut samples: Vec<Sample>) {
        let mut last_chunk = self
            .last_chunk
            .lock()
            .expect("Self::len can not poison this lock");
        std::mem::swap(&mut *last_chunk, &mut samples);
    }

    fn push_normal(&self, samples: Vec<Sample>) {
        let _pushed_out_of_ringbuf = self.inner.force_push(samples);
    }
}

/// constant source, only works on a single span
pub struct ProcessBuffer<const N: usize, S, F>
where
    S: Source + Sized,
    F: FnMut(&mut [Sample; N]),
{
    pub(super) inner: S,
    pub(super) callback: F,
    /// Buffer used for both input and output.
    pub(super) buffer: [Sample; N],
    /// Next already processed sample is at this index
    /// in buffer.
    ///
    /// If this is equal to the length of the buffer we have no more samples and
    /// we must get new ones and process them
    pub(super) next: usize,
}

impl<const N: usize, S, F> Iterator for ProcessBuffer<N, S, F>
where
    S: Source + Sized,
    F: FnMut(&mut [Sample; N]),
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        self.next += 1;
        if self.next < self.buffer.len() {
            let sample = self.buffer[self.next];
            return Some(sample);
        }

        for sample in &mut self.buffer {
            *sample = self.inner.next()?
        }
        (self.callback)(&mut self.buffer);

        self.next = 0;
        Some(self.buffer[0])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<const N: usize, S, F> Source for ProcessBuffer<N, S, F>
where
    S: Source + Sized,
    F: FnMut(&mut [Sample; N]),
{
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

/// constant source, only works on a single span
pub struct InspectBuffer<const N: usize, S, F>
where
    S: Source + Sized,
    F: FnMut(&[Sample; N]),
{
    pub(super) inner: S,
    pub(super) callback: F,
    /// Stores already emitted samples, once its full we call the callback.
    pub(super) buffer: [Sample; N],
    /// Next free element in buffer. If this is equal to the buffer length
    /// we have no more free elements.
    pub(super) free: usize,
}

impl<const N: usize, S, F> Iterator for InspectBuffer<N, S, F>
where
    S: Source + Sized,
    F: FnMut(&[Sample; N]),
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(sample) = self.inner.next() else {
            return None;
        };

        self.buffer[self.free] = sample;
        self.free += 1;

        if self.free == self.buffer.len() {
            (self.callback)(&self.buffer);
            self.free = 0
        }

        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<const N: usize, S, F> Source for InspectBuffer<N, S, F>
where
    S: Source + Sized,
    F: FnMut(&[Sample; N]),
{
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

/// constant source, only works on a single span
#[derive(Debug)]
pub struct Replayable<S: Source> {
    pub(super) inner: S,
    pub(super) buffer: Vec<Sample>,
    pub(super) chunk_size: usize,
    pub(super) tx: Arc<ReplayQueue>,
    pub(super) is_active: Arc<AtomicBool>,
}

impl<S: Source> Iterator for Replayable<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sample) = self.inner.next() {
            self.buffer.push(sample);
            // If the buffer is full send it
            if self.buffer.len() == self.chunk_size {
                self.tx.push_normal(std::mem::take(&mut self.buffer));
            }
            Some(sample)
        } else {
            let last_chunk = std::mem::take(&mut self.buffer);
            self.tx.push_last(last_chunk);
            self.is_active.store(false, Ordering::Relaxed);
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for Replayable<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// constant source, only works on a single span
#[derive(Debug)]
pub struct Replay {
    pub(super) rx: Arc<ReplayQueue>,
    pub(super) buffer: std::vec::IntoIter<Sample>,
    pub(super) sleep_duration: Duration,
    pub(super) sample_rate: SampleRate,
    pub(super) channel_count: ChannelCount,
    pub(super) source_is_active: Arc<AtomicBool>,
}

impl Replay {
    pub fn source_is_active(&self) -> bool {
        // - source could return None and not drop
        // - source could be dropped before returning None
        self.source_is_active.load(Ordering::Relaxed) && Arc::strong_count(&self.rx) < 2
    }

    /// Duration of what is in the buffer and can be returned without blocking.
    pub fn duration_ready(&self) -> Duration {
        let samples_per_second = self.channels().get() as u32 * self.sample_rate().get();

        let seconds_queued = self.samples_ready() as f64 / samples_per_second as f64;
        Duration::from_secs_f64(seconds_queued)
    }

    /// Number of samples in the buffer and can be returned without blocking.
    pub fn samples_ready(&self) -> usize {
        self.rx.len() + self.buffer.len()
    }
}

impl Iterator for Replay {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sample) = self.buffer.next() {
            return Some(sample);
        }

        loop {
            if let Some(new_buffer) = self.rx.pop() {
                self.buffer = new_buffer.into_iter();
                return self.buffer.next();
            }

            if !self.source_is_active() {
                return None;
            }

            // The queue does not support blocking on a next item. We want this queue as it
            // is quite fast and provides a fixed size. We know how many samples are in a
            // buffer so if we do not get one now we must be getting one after `sleep_duration`.
            std::thread::sleep(self.sleep_duration);
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        ((self.rx.len() + self.buffer.len()), None)
    }
}

impl Source for Replay {
    fn current_span_len(&self) -> Option<usize> {
        None // source is not compatible with spans
    }

    fn channels(&self) -> ChannelCount {
        self.channel_count
    }

    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

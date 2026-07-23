use std::{
    num::NonZero,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crossbeam::queue::ArrayQueue;
use log::warn;
use rodio::{
    ChannelCount, Sample, SampleRate, Source, conversions::SampleRateConverter, nz,
    source::UniformSourceIterator,
};

const MAX_CHANNELS: usize = 8;

#[derive(Debug, thiserror::Error)]
#[error("Replay duration is too short must be >= 100ms")]
pub struct ReplayDurationTooShort;

#[path = "rodio_ext/adapters.rs"]
mod adapters;
use adapters::ReplayQueue;
pub use adapters::{InspectBuffer, ProcessBuffer, Replay, Replayable};

// These all require constant sources (so the span is infinitely long)
// this is not guaranteed by rodio however we know it to be true in all our
// applications. Rodio desperately needs a constant source concept.
pub trait RodioExt: Source + Sized {
    fn process_buffer<const N: usize, F>(self, callback: F) -> ProcessBuffer<N, Self, F>
    where
        F: FnMut(&mut [Sample; N]);
    fn inspect_buffer<const N: usize, F>(self, callback: F) -> InspectBuffer<N, Self, F>
    where
        F: FnMut(&[Sample; N]);
    fn replayable(
        self,
        duration: Duration,
    ) -> Result<(Replay, Replayable<Self>), ReplayDurationTooShort>;
    fn take_samples(self, n: usize) -> TakeSamples<Self>;
    fn constant_params(
        self,
        channel_count: ChannelCount,
        sample_rate: SampleRate,
    ) -> UniformSourceIterator<Self>;
    fn constant_samplerate(self, sample_rate: SampleRate) -> ConstantSampleRate<Self>;
    fn possibly_disconnected_channels_to_mono(self) -> ToMono<Self>;
}

impl<S: Source> RodioExt for S {
    fn process_buffer<const N: usize, F>(self, callback: F) -> ProcessBuffer<N, Self, F>
    where
        F: FnMut(&mut [Sample; N]),
    {
        ProcessBuffer {
            inner: self,
            callback,
            buffer: [0.0; N],
            next: N,
        }
    }
    fn inspect_buffer<const N: usize, F>(self, callback: F) -> InspectBuffer<N, Self, F>
    where
        F: FnMut(&[Sample; N]),
    {
        InspectBuffer {
            inner: self,
            callback,
            buffer: [0.0; N],
            free: 0,
        }
    }
    /// Maintains a live replay with a history of at least `duration` seconds.
    ///
    /// Note:
    /// History can be 100ms longer if the source drops before or while the
    /// replay is being read
    ///
    /// # Errors
    /// If duration is smaller than 100ms
    fn replayable(
        self,
        duration: Duration,
    ) -> Result<(Replay, Replayable<Self>), ReplayDurationTooShort> {
        if duration < Duration::from_millis(100) {
            return Err(ReplayDurationTooShort);
        }

        let samples_per_second = self.sample_rate().get() as usize * self.channels().get() as usize;
        let samples_to_queue = duration.as_secs_f64() * samples_per_second as f64;
        let samples_to_queue =
            (samples_to_queue as usize).next_multiple_of(self.channels().get().into());

        let chunk_size =
            (samples_per_second.div_ceil(10)).next_multiple_of(self.channels().get() as usize);
        let chunks_to_queue = samples_to_queue.div_ceil(chunk_size);

        let is_active = Arc::new(AtomicBool::new(true));
        let queue = Arc::new(ReplayQueue::new(chunks_to_queue, chunk_size));
        Ok((
            Replay {
                rx: Arc::clone(&queue),
                buffer: Vec::new().into_iter(),
                sleep_duration: duration / 2,
                sample_rate: self.sample_rate(),
                channel_count: self.channels(),
                source_is_active: is_active.clone(),
            },
            Replayable {
                tx: queue,
                inner: self,
                buffer: Vec::with_capacity(chunk_size),
                chunk_size,
                is_active,
            },
        ))
    }
    fn take_samples(self, n: usize) -> TakeSamples<S> {
        TakeSamples {
            inner: self,
            left_to_take: n,
        }
    }
    fn constant_params(
        self,
        channel_count: ChannelCount,
        sample_rate: SampleRate,
    ) -> UniformSourceIterator<Self> {
        UniformSourceIterator::new(self, channel_count, sample_rate)
    }
    fn constant_samplerate(self, sample_rate: SampleRate) -> ConstantSampleRate<Self> {
        ConstantSampleRate::new(self, sample_rate)
    }
    fn possibly_disconnected_channels_to_mono(self) -> ToMono<Self> {
        ToMono::new(self)
    }
}

pub struct ConstantSampleRate<S: Source> {
    inner: SampleRateConverter<S>,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

impl<S: Source> ConstantSampleRate<S> {
    fn new(source: S, target_rate: SampleRate) -> Self {
        let input_sample_rate = source.sample_rate();
        let channels = source.channels();
        let inner = SampleRateConverter::new(source, input_sample_rate, target_rate, channels);
        Self {
            inner,
            channels,
            sample_rate: target_rate,
        }
    }
}

impl<S: Source> Iterator for ConstantSampleRate<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for ConstantSampleRate<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None // not supported (not used by us)
    }
}

const TYPICAL_NOISE_FLOOR: Sample = 1e-3;

/// constant source, only works on a single span
pub struct ToMono<S> {
    inner: S,
    input_channel_count: ChannelCount,
    connected_channels: ChannelCount,
    /// running mean of second channel 'volume'
    means: [f32; MAX_CHANNELS],
}
impl<S: Source> ToMono<S> {
    fn new(input: S) -> Self {
        let channels = input
            .channels()
            .min(const { NonZero::<u16>::new(MAX_CHANNELS as u16).unwrap() });
        if channels < input.channels() {
            warn!("Ignoring input channels {}..", channels.get());
        }

        Self {
            connected_channels: channels,
            input_channel_count: channels,
            inner: input,
            means: [TYPICAL_NOISE_FLOOR; MAX_CHANNELS],
        }
    }
}

impl<S: Source> Source for ToMono<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        rodio::nz!(1)
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

fn update_mean(mean: &mut f32, sample: Sample) {
    const HISTORY: f32 = 500.0;
    *mean *= (HISTORY - 1.0) / HISTORY;
    *mean += sample.abs() / HISTORY;
}

impl<S: Source> Iterator for ToMono<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let mut mono_sample = 0f32;
        let mut active_channels = 0;
        for channel in 0..self.input_channel_count.get() as usize {
            let sample = self.inner.next()?;
            mono_sample += sample;

            update_mean(&mut self.means[channel], sample);
            if self.means[channel] > TYPICAL_NOISE_FLOOR / 10.0 {
                active_channels += 1;
            }
        }
        mono_sample /= self.connected_channels.get() as f32;
        self.connected_channels = NonZero::new(active_channels).unwrap_or(nz!(1));

        Some(mono_sample)
    }
}

/// constant source, only works on a single span
pub struct TakeSamples<S> {
    inner: S,
    left_to_take: usize,
}

impl<S: Source> Iterator for TakeSamples<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.left_to_take == 0 {
            None
        } else {
            self.left_to_take -= 1;
            self.inner.next()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.left_to_take))
    }
}

impl<S: Source> Source for TakeSamples<S> {
    fn current_span_len(&self) -> Option<usize> {
        None // does not support spans
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.left_to_take as f64
                / self.sample_rate().get() as f64
                / self.channels().get() as f64,
        ))
    }
}

#[cfg(test)]
mod tests {
    use rodio::{nz, static_buffer::StaticSamplesBuffer};

    use super::*;

    const SAMPLES: [Sample; 5] = [0.0, 1.0, 2.0, 3.0, 4.0];

    fn test_source() -> StaticSamplesBuffer {
        StaticSamplesBuffer::new(nz!(1), nz!(1), &SAMPLES)
    }

    mod process_buffer {
        use super::*;

        #[test]
        fn callback_gets_all_samples() {
            let input = test_source();

            let _ = input
                .process_buffer::<{ SAMPLES.len() }, _>(|buffer| assert_eq!(*buffer, SAMPLES))
                .count();
        }
        #[test]
        fn callback_modifies_yielded() {
            let input = test_source();

            let yielded: Vec<_> = input
                .process_buffer::<{ SAMPLES.len() }, _>(|buffer| {
                    for sample in buffer {
                        *sample += 1.0;
                    }
                })
                .collect();
            assert_eq!(
                yielded,
                SAMPLES.into_iter().map(|s| s + 1.0).collect::<Vec<_>>()
            )
        }
        #[test]
        fn source_truncates_to_whole_buffers() {
            let input = test_source();

            let yielded = input
                .process_buffer::<3, _>(|buffer| assert_eq!(buffer, &SAMPLES[..3]))
                .count();
            assert_eq!(yielded, 3)
        }
    }

    mod inspect_buffer {
        use super::*;

        #[test]
        fn callback_gets_all_samples() {
            let input = test_source();

            let _ = input
                .inspect_buffer::<{ SAMPLES.len() }, _>(|buffer| assert_eq!(*buffer, SAMPLES))
                .count();
        }
        #[test]
        fn source_does_not_truncate() {
            let input = test_source();

            let yielded = input
                .inspect_buffer::<3, _>(|buffer| assert_eq!(buffer, &SAMPLES[..3]))
                .count();
            assert_eq!(yielded, SAMPLES.len())
        }
    }

    mod instant_replay {
        use super::*;

        #[test]
        fn continues_after_history() {
            let input = test_source();

            let (mut replay, mut source) = input
                .replayable(Duration::from_secs(3))
                .expect("longer than 100ms");

            source.by_ref().take(3).count();
            let yielded: Vec<Sample> = replay.by_ref().take(3).collect();
            assert_eq!(&yielded, &SAMPLES[0..3],);

            source.count();
            let yielded: Vec<Sample> = replay.collect();
            assert_eq!(&yielded, &SAMPLES[3..5],);
        }

        #[test]
        fn keeps_only_latest() {
            let input = test_source();

            let (mut replay, mut source) = input
                .replayable(Duration::from_secs(2))
                .expect("longer than 100ms");

            source.by_ref().take(5).count(); // get all items but do not end the source
            let yielded: Vec<Sample> = replay.by_ref().take(2).collect();
            assert_eq!(&yielded, &SAMPLES[3..5]);
            source.count(); // exhaust source
            assert_eq!(replay.next(), None);
        }

        #[test]
        fn keeps_correct_amount_of_seconds() {
            let input = StaticSamplesBuffer::new(nz!(1), nz!(16_000), &[0.0; 40_000]);

            let (replay, mut source) = input
                .replayable(Duration::from_secs(2))
                .expect("longer than 100ms");

            // exhaust but do not yet end source
            source.by_ref().take(40_000).count();

            // take all samples we can without blocking
            let ready = replay.samples_ready();
            let n_yielded = replay.take_samples(ready).count();

            let max = source.sample_rate().get() * source.channels().get() as u32 * 2;
            let margin = 16_000 / 10; // 100ms
            assert!(n_yielded as u32 >= max - margin);
        }

        #[test]
        fn samples_ready() {
            let input = StaticSamplesBuffer::new(nz!(1), nz!(16_000), &[0.0; 40_000]);
            let (mut replay, source) = input
                .replayable(Duration::from_secs(2))
                .expect("longer than 100ms");
            assert_eq!(replay.by_ref().samples_ready(), 0);

            source.take(8000).count(); // half a second
            let margin = 16_000 / 10; // 100ms
            let ready = replay.samples_ready();
            assert!(ready >= 8000 - margin);
        }
    }
}

//! Host audio access for ARA sources.
//!
//! The host runtime asks an [`AudioAccessProvider`] for independent readers
//! whenever a plugin wants to inspect source samples. [`MemoryAudioAccess`] is
//! the default provider: it stores immutable planar `f32` buffers keyed by the
//! stable [`AudioSourceId`] token exposed by [`crate::AraDocument`].

use ara2_bridge_core::AraError;
use ara2_bridge_host::{AudioAccessProvider, AudioSourceId, HostAudioReader};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Immutable planar audio held by a host audio source.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSourceBuffer {
    channels: Vec<Vec<f32>>,
}

impl AudioSourceBuffer {
    /// Builds a buffer from planar channels.
    ///
    /// # Errors
    ///
    /// Returns [`AraError::InvalidArgument`] when there are no channels or the
    /// channels have different lengths.
    pub fn new(channels: Vec<Vec<f32>>) -> Result<Self, AraError> {
        if channels.is_empty() {
            return Err(AraError::InvalidArgument("audio source has no channels"));
        }
        let sample_count = channels[0].len();
        if channels.iter().any(|channel| channel.len() != sample_count) {
            return Err(AraError::InvalidArgument(
                "audio source channels have different lengths",
            ));
        }
        Ok(Self { channels })
    }

    /// Builds a mono/stereo buffer from slices.
    #[must_use]
    pub fn from_channels(channels: &[&[f32]]) -> Self {
        Self {
            channels: channels.iter().map(|channel| channel.to_vec()).collect(),
        }
    }

    /// Returns the number of samples per channel.
    #[must_use]
    pub fn sample_count(&self) -> i64 {
        i64::try_from(self.channels.first().map_or(0, Vec::len)).unwrap_or(i64::MAX)
    }

    /// Returns the number of channels.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Returns one channel, if present.
    #[must_use]
    pub fn channel(&self, index: usize) -> Option<&[f32]> {
        self.channels.get(index).map(Vec::as_slice)
    }

    fn sample(&self, channel: usize, position: i64) -> f32 {
        let Some(samples) = self.channels.get(channel) else {
            return 0.0;
        };
        let Ok(index) = usize::try_from(position) else {
            return 0.0;
        };
        samples.get(index).copied().unwrap_or(0.0)
    }
}

/// Default in-memory audio-access provider.
///
/// Register a buffer with [`MemoryAudioAccess::insert`] using the ID returned
/// by [`crate::AraDocument::audio_source_id`] after creating the matching ARA
/// audio source.
#[derive(Debug, Default)]
pub struct MemoryAudioAccess {
    sources: Mutex<HashMap<AudioSourceId, Arc<AudioSourceBuffer>>>,
}

impl MemoryAudioAccess {
    /// Creates an empty provider.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces the buffer for `source`.
    pub fn insert(&self, source: AudioSourceId, buffer: Arc<AudioSourceBuffer>) {
        lock(&self.sources).insert(source, buffer);
    }

    /// Removes the buffer for `source`.
    pub fn remove(&self, source: AudioSourceId) -> Option<Arc<AudioSourceBuffer>> {
        lock(&self.sources).remove(&source)
    }

    /// Returns the registered buffer for `source`.
    #[must_use]
    pub fn get(&self, source: AudioSourceId) -> Option<Arc<AudioSourceBuffer>> {
        lock(&self.sources).get(&source).cloned()
    }

    /// Removes every registered buffer.
    pub fn clear(&self) {
        lock(&self.sources).clear();
    }
}

impl AudioAccessProvider for MemoryAudioAccess {
    fn create_reader(
        &self,
        source: AudioSourceId,
        _use_64_bit_samples: bool,
    ) -> Result<Box<dyn HostAudioReader>, AraError> {
        let buffer = self
            .get(source)
            .ok_or(AraError::InvalidArgument("unknown ARA audio source"))?;
        Ok(Box::new(MemoryAudioReader { buffer }))
    }
}

struct MemoryAudioReader {
    buffer: Arc<AudioSourceBuffer>,
}

impl HostAudioReader for MemoryAudioReader {
    fn channel_count(&self) -> usize {
        self.buffer.channel_count()
    }

    fn sample_count(&self) -> i64 {
        self.buffer.sample_count()
    }

    fn read_f32(
        &mut self,
        sample_position: i64,
        buffers: &mut [&mut [f32]],
    ) -> Result<(), AraError> {
        if buffers.len() != self.buffer.channel_count() {
            return Err(AraError::InvalidArgument(
                "audio reader buffer count does not match source channels",
            ));
        }
        let sample_count = self.buffer.sample_count();
        for (channel_index, output) in buffers.iter_mut().enumerate() {
            for (frame, sample) in output.iter_mut().enumerate() {
                let offset = i64::try_from(frame).unwrap_or(i64::MAX);
                let position = sample_position.checked_add(offset).unwrap_or(i64::MAX);
                *sample = if position < sample_count {
                    self.buffer.sample(channel_index, position)
                } else {
                    0.0
                };
            }
        }
        Ok(())
    }

    fn read_f64(
        &mut self,
        sample_position: i64,
        buffers: &mut [&mut [f64]],
    ) -> Result<(), AraError> {
        if buffers.len() != self.buffer.channel_count() {
            return Err(AraError::InvalidArgument(
                "audio reader buffer count does not match source channels",
            ));
        }
        let sample_count = self.buffer.sample_count();
        for (channel_index, output) in buffers.iter_mut().enumerate() {
            for (frame, sample) in output.iter_mut().enumerate() {
                let offset = i64::try_from(frame).unwrap_or(i64::MAX);
                let position = sample_position.checked_add(offset).unwrap_or(i64::MAX);
                *sample = if position < sample_count {
                    f64::from(self.buffer.sample(channel_index, position))
                } else {
                    0.0
                };
            }
        }
        Ok(())
    }
}

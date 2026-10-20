// fork/cloud_chunker.rs
//
// Regroups the live VAD segments into requests sized for cloud STT. Local engines get
// every VAD segment as-is; for a cloud model, many 1-3 s requests mean more latency,
// cost and worse accuracy, while unbounded segments (continuous speech, issue #756)
// can exceed the request limits. Consecutive segments are merged, padding the real
// silence between them with zeros so `timestamp + len / sample_rate` stays exact.

use crate::audio::AudioChunk;

#[derive(Debug, Clone, Copy)]
pub struct ChunkingPolicy {
    /// Dispatch as soon as the merged chunk reaches this length.
    pub target_secs: f64,
    /// Below this, keep waiting for more speech (unless a long gap or idle flush).
    pub min_secs: f64,
    /// Never send more than this in one request.
    pub max_secs: f64,
    /// A longer silence between two segments closes the current chunk.
    pub max_gap_secs: f64,
    /// Flush the pending chunk after this long without new speech.
    pub idle_flush_ms: u64,
}

impl Default for ChunkingPolicy {
    fn default() -> Self {
        Self { target_secs: 10.0, min_secs: 1.5, max_secs: 28.0, max_gap_secs: 1.2, idle_flush_ms: 1500 }
    }
}

struct Pending {
    chunk: AudioChunk,
    label: Option<String>,
}

impl Pending {
    fn duration(&self) -> f64 {
        self.chunk.data.len() as f64 / self.chunk.sample_rate as f64
    }
    fn end(&self) -> f64 {
        self.chunk.timestamp + self.duration()
    }
}

pub struct ChunkAccumulator {
    policy: ChunkingPolicy,
    pending: Option<Pending>,
}

impl ChunkAccumulator {
    pub fn new(policy: ChunkingPolicy) -> Self {
        Self { policy, pending: None }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Add one VAD segment (with its dominant source label, if known).
    /// Returns the chunks that are ready to be transcribed, in audio order.
    pub fn push(&mut self, chunk: AudioChunk, label: Option<String>) -> Vec<AudioChunk> {
        let mut ready = Vec::new();
        if chunk.data.is_empty() || chunk.sample_rate == 0 {
            return ready;
        }

        if let Some(pending) = self.pending.as_mut() {
            let gap = (chunk.timestamp - pending.end()).max(0.0);
            let incoming = chunk.data.len() as f64 / chunk.sample_rate as f64;
            let speaker_changed = label.is_some() && pending.label.is_some() && label != pending.label;
            let close_current = chunk.sample_rate != pending.chunk.sample_rate
                || gap > self.policy.max_gap_secs
                || (speaker_changed && pending.duration() >= self.policy.min_secs)
                || pending.duration() + gap + incoming > self.policy.max_secs;

            if close_current {
                ready.extend(self.take_pending());
            } else {
                let pad = (gap * pending.chunk.sample_rate as f64).round() as usize;
                pending.chunk.data.extend(std::iter::repeat(0.0).take(pad));
                pending.chunk.data.extend_from_slice(&chunk.data);
                if pending.label != label {
                    pending.label = None;
                }
                if pending.duration() >= self.policy.target_secs {
                    ready.extend(self.take_pending());
                }
                return ready;
            }
        }

        self.pending = Some(Pending { chunk, label });
        let too_long = self.pending.as_ref().map(|p| p.duration() >= self.policy.target_secs).unwrap_or(false);
        if too_long {
            ready.extend(self.take_pending());
        }
        ready
    }

    /// Flush whatever is pending (idle timeout or end of recording).
    pub fn flush(&mut self) -> Vec<AudioChunk> {
        self.take_pending()
    }

    fn take_pending(&mut self) -> Vec<AudioChunk> {
        match self.pending.take() {
            Some(p) => split_long(p.chunk, self.policy.max_secs),
            None => Vec::new(),
        }
    }
}

/// Split a chunk longer than `max_secs` at low-energy points (reuses the batch splitter).
fn split_long(chunk: AudioChunk, max_secs: f64) -> Vec<AudioChunk> {
    let max_samples = (max_secs * chunk.sample_rate as f64) as usize;
    if chunk.data.len() <= max_samples || chunk.sample_rate != 16000 {
        return vec![chunk];
    }
    let duration_ms = chunk.data.len() as f64 / chunk.sample_rate as f64 * 1000.0;
    let segment = crate::audio::vad::SpeechSegment {
        samples: chunk.data,
        start_timestamp_ms: chunk.timestamp * 1000.0,
        end_timestamp_ms: chunk.timestamp * 1000.0 + duration_ms,
        confidence: 1.0,
    };
    crate::audio::common::split_segment_at_silence(&segment, max_samples)
        .into_iter()
        .map(|s| AudioChunk {
            data: s.samples,
            sample_rate: chunk.sample_rate,
            timestamp: s.start_timestamp_ms / 1000.0,
            chunk_id: chunk.chunk_id,
            device_type: chunk.device_type.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::recording_state::DeviceType;

    const SR: u32 = 16000;

    fn seg(start: f64, secs: f64, id: u64) -> AudioChunk {
        AudioChunk {
            data: vec![0.1; (secs * SR as f64) as usize],
            sample_rate: SR,
            timestamp: start,
            chunk_id: id,
            device_type: DeviceType::Microphone,
        }
    }

    fn secs(c: &AudioChunk) -> f64 {
        c.data.len() as f64 / c.sample_rate as f64
    }

    #[test]
    fn merges_close_segments_until_target_and_pads_gaps() {
        let mut acc = ChunkAccumulator::new(ChunkingPolicy::default());
        assert!(acc.push(seg(0.0, 4.0, 0), None).is_empty());
        assert!(acc.push(seg(4.5, 4.0, 1), None).is_empty());
        let out = acc.push(seg(9.0, 2.0, 2), None);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].timestamp, 0.0);
        // 4 + 0.5 gap + 4 + 0.5 gap + 2 = 11 s, end time stays exact
        assert!((secs(&out[0]) - 11.0).abs() < 1e-3);
        assert!(!acc.has_pending());
    }

    #[test]
    fn long_gap_closes_the_pending_chunk() {
        let mut acc = ChunkAccumulator::new(ChunkingPolicy::default());
        acc.push(seg(0.0, 3.0, 0), None);
        let out = acc.push(seg(6.0, 3.0, 1), None);
        assert_eq!(out.len(), 1);
        assert!((secs(&out[0]) - 3.0).abs() < 1e-3);
        assert!(acc.has_pending());
        let rest = acc.flush();
        assert_eq!(rest[0].timestamp, 6.0);
    }

    #[test]
    fn speaker_change_closes_chunk_once_min_reached() {
        let mut acc = ChunkAccumulator::new(ChunkingPolicy::default());
        acc.push(seg(0.0, 2.0, 0), Some("mic".into()));
        let out = acc.push(seg(2.3, 2.0, 1), Some("system".into()));
        assert_eq!(out.len(), 1);
        assert!((secs(&out[0]) - 2.0).abs() < 1e-3);
    }

    #[test]
    fn never_exceeds_max_seconds() {
        let mut acc = ChunkAccumulator::new(ChunkingPolicy::default());
        let out = acc.push(seg(0.0, 65.0, 0), None);
        assert!(out.len() >= 3);
        assert!(out.iter().all(|c| secs(c) <= 28.0 + 1e-3));
        assert_eq!(out[0].timestamp, 0.0);
        assert!(out.windows(2).all(|w| w[1].timestamp > w[0].timestamp));
    }

    #[test]
    fn short_isolated_segment_waits_for_idle_flush() {
        let mut acc = ChunkAccumulator::new(ChunkingPolicy::default());
        assert!(acc.push(seg(0.0, 1.0, 0), None).is_empty());
        assert!(acc.has_pending());
        assert_eq!(acc.flush().len(), 1);
        assert!(acc.flush().is_empty());
    }
}

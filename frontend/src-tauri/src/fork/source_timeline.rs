// fork/source_timeline.rs
//
// Per-window energy of the microphone and system-audio streams, recorded just before
// they are mixed (audio/pipeline.rs). Mixer windows are fixed-size and back to back,
// so window i covers [i·w, (i+1)·w) on the VAD/transcript timeline. That lets any
// transcript segment be labelled "mic" (the user) or "system" (remote participants)
// without un-mixing the audio.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub const SPEAKER_MIC: &str = "mic";
pub const SPEAKER_SYSTEM: &str = "system";
pub const SPEAKER_MIXED: &str = "mixed";

/// Below this mean-square energy (~ -50 dBFS) a window counts as silence.
const SILENCE_MEAN_SQUARE: f64 = 1e-5;
/// Dominance needed to attribute a segment to one source.
const DOMINANCE_DB: f64 = 6.0;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceTimeline {
    pub window_secs: f64,
    /// (mic mean square, system mean square) per mixer window
    pub windows: Vec<(f32, f32)>,
}

static TIMELINE: Lazy<Mutex<SourceTimeline>> = Lazy::new(|| Mutex::new(SourceTimeline::default()));

/// Start a new recording session.
pub fn reset() {
    if let Ok(mut t) = TIMELINE.lock() {
        *t = SourceTimeline::default();
    }
}

/// Record one mixer window (called once per window, in mixing order).
pub fn push_window(mic: &[f32], sys: &[f32], sample_rate: u32) {
    if let Ok(mut t) = TIMELINE.lock() {
        if t.window_secs == 0.0 && sample_rate > 0 {
            t.window_secs = mic.len().max(sys.len()) as f64 / sample_rate as f64;
        }
        t.windows.push((mean_square(mic), mean_square(sys)));
    }
}

/// Dominant source over [start, end) seconds of the live timeline.
pub fn label(start: f64, end: f64) -> Option<String> {
    TIMELINE.lock().ok().and_then(|t| t.label(start, end))
}

pub fn snapshot() -> SourceTimeline {
    TIMELINE.lock().map(|t| t.clone()).unwrap_or_default()
}

/// Persist the timeline next to the recording, for the end-of-meeting diarization
/// pass (it identifies which diarized speaker is the user).
pub fn save_to_folder(folder: &Path) -> std::io::Result<()> {
    let timeline = snapshot();
    if timeline.windows.is_empty() {
        return Ok(());
    }
    let json = serde_json::to_string(&timeline)?;
    std::fs::write(folder.join("source_energy.json"), json)
}

pub fn load_from_folder(folder: &Path) -> Option<SourceTimeline> {
    let raw = std::fs::read_to_string(folder.join("source_energy.json")).ok()?;
    serde_json::from_str(&raw).ok()
}

impl SourceTimeline {
    pub fn label(&self, start: f64, end: f64) -> Option<String> {
        let (mic, sys) = self.energy(start, end)?;
        let ratio_db = 10.0 * ((mic + 1e-12) / (sys + 1e-12)).log10();
        Some(
            if ratio_db >= DOMINANCE_DB {
                SPEAKER_MIC
            } else if ratio_db <= -DOMINANCE_DB {
                SPEAKER_SYSTEM
            } else {
                SPEAKER_MIXED
            }
            .to_string(),
        )
    }

    /// Summed (mic, system) energy of the non-silent windows overlapping [start, end).
    pub fn energy(&self, start: f64, end: f64) -> Option<(f64, f64)> {
        if self.window_secs <= 0.0 || self.windows.is_empty() || end <= start {
            return None;
        }
        let first = (start / self.window_secs).floor().max(0.0) as usize;
        let last = ((end / self.window_secs).ceil() as usize).min(self.windows.len());
        if first >= last {
            return None;
        }
        let (mut mic, mut sys) = (0.0f64, 0.0f64);
        for &(m, s) in &self.windows[first..last] {
            let (m, s) = (m as f64, s as f64);
            if m.max(s) < SILENCE_MEAN_SQUARE {
                continue;
            }
            mic += m;
            sys += s;
        }
        if mic + sys == 0.0 {
            None
        } else {
            Some((mic, sys))
        }
    }

    /// Share of the meeting's speech energy that came from the microphone.
    pub fn mic_share(&self) -> Option<f64> {
        let duration = self.windows.len() as f64 * self.window_secs;
        let (mic, sys) = self.energy(0.0, duration)?;
        Some(mic / (mic + sys))
    }
}

fn mean_square(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|&x| x * x).sum::<f32>() / samples.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline(windows: Vec<(f32, f32)>) -> SourceTimeline {
        SourceTimeline { window_secs: 0.6, windows }
    }

    #[test]
    fn labels_dominant_source() {
        let t = timeline(vec![(0.05, 0.0001), (0.05, 0.0001), (0.0001, 0.04), (0.0001, 0.04)]);
        assert_eq!(t.label(0.0, 1.2).as_deref(), Some(SPEAKER_MIC));
        assert_eq!(t.label(1.2, 2.4).as_deref(), Some(SPEAKER_SYSTEM));
        assert_eq!(t.label(0.0, 2.4).as_deref(), Some(SPEAKER_MIXED));
    }

    #[test]
    fn silence_and_out_of_range_give_no_label() {
        let t = timeline(vec![(0.000001, 0.000001); 4]);
        assert_eq!(t.label(0.0, 2.4), None);
        assert_eq!(t.label(10.0, 12.0), None);
        assert_eq!(timeline(vec![]).label(0.0, 1.0), None);
    }

    #[test]
    fn partial_windows_are_included() {
        // segment [0.5, 0.7) touches windows 0 and 1
        let t = timeline(vec![(0.000001, 0.000001), (0.03, 0.0001)]);
        assert_eq!(t.label(0.5, 0.7).as_deref(), Some(SPEAKER_MIC));
    }

    #[test]
    fn mic_share_over_whole_meeting() {
        let t = timeline(vec![(0.03, 0.0), (0.0, 0.01)]);
        let share = t.mic_share().unwrap();
        assert!((share - 0.75).abs() < 1e-6);
    }
}

// fork/diarization/align.rs
//
// Pure logic of the diarization pass: merge per-part word lists (reconciling speaker
// ids across overlapping parts), rebuild transcript segments from words, and find
// which diarized speaker is the user (mic side of the live energy timeline).

use crate::fork::source_timeline::SourceTimeline;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct DiarWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
    /// Provider speaker key ("speaker_0"), local to its part until merged.
    pub speaker: Option<String>,
}

impl DiarWord {
    fn mid(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// Words of one transcription request; times are relative to `offset`.
#[derive(Debug, Clone)]
pub struct PartWords {
    pub offset: f64,
    pub duration: f64,
    pub words: Vec<DiarWord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiarSegment {
    pub speaker: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Merge parts into one word list with global speaker keys ("speaker_0", ...).
/// Consecutive parts overlap; speakers are matched by who talks at the same time in the
/// overlap, then each side keeps its words up to the middle of the overlap.
pub fn merge_parts(mut parts: Vec<PartWords>) -> Vec<DiarWord> {
    parts.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<DiarWord> = Vec::new();
    let mut next_global = 0usize;
    let mut previous_end: Option<f64> = None;

    for part in parts {
        let words: Vec<DiarWord> = part
            .words
            .iter()
            .filter(|w| !w.text.trim().is_empty())
            .map(|w| DiarWord { start: w.start + part.offset, end: w.end + part.offset, ..w.clone() })
            .collect();

        let mut mapping: HashMap<String, String> = HashMap::new();
        let boundary = match previous_end {
            Some(prev_end) if prev_end > part.offset => {
                // Co-occurrence of (local, global) speakers inside the overlap
                let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
                for w in words.iter().filter(|w| w.mid() <= prev_end) {
                    let Some(local) = &w.speaker else { continue };
                    let best = merged
                        .iter()
                        .filter(|p| p.speaker.is_some() && p.mid() >= part.offset - 1.0)
                        .min_by(|a, b| {
                            (a.mid() - w.mid()).abs().partial_cmp(&(b.mid() - w.mid()).abs()).unwrap_or(std::cmp::Ordering::Equal)
                        });
                    if let Some(p) = best.filter(|p| (p.mid() - w.mid()).abs() < 0.5) {
                        *counts.entry((local.clone(), p.speaker.clone().unwrap())).or_default() += 1;
                    }
                }
                let mut pairs: Vec<((String, String), usize)> = counts.into_iter().collect();
                pairs.sort_by(|a, b| b.1.cmp(&a.1));
                let mut used_global: HashSet<String> = HashSet::new();
                for ((local, global), _) in pairs {
                    if mapping.contains_key(&local) || used_global.contains(&global) {
                        continue;
                    }
                    used_global.insert(global.clone());
                    mapping.insert(local, global);
                }
                (part.offset + prev_end) / 2.0
            }
            _ => part.offset,
        };

        merged.retain(|w| w.mid() < boundary);
        for mut w in words.into_iter().filter(|w| w.mid() >= boundary) {
            if let Some(local) = w.speaker.take() {
                let global = mapping
                    .entry(local)
                    .or_insert_with(|| {
                        let key = format!("speaker_{}", next_global);
                        next_global += 1;
                        key
                    })
                    .clone();
                w.speaker = Some(global);
            }
            merged.push(w);
        }
        previous_end = Some(part.offset + part.duration);
    }

    merged.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    renumber_by_first_appearance(&mut merged);
    merged
}

/// "speaker_0" is whoever speaks first, "speaker_1" the next new voice, and so on.
fn renumber_by_first_appearance(words: &mut [DiarWord]) {
    let mut order: HashMap<String, String> = HashMap::new();
    for w in words.iter() {
        if let Some(s) = &w.speaker {
            if !order.contains_key(s) {
                let key = format!("speaker_{}", order.len());
                order.insert(s.clone(), key);
            }
        }
    }
    for w in words.iter_mut() {
        if let Some(s) = &w.speaker {
            w.speaker = order.get(s).cloned();
        }
    }
}

const MAX_GAP_SECS: f64 = 1.2;
const MAX_SEGMENT_SECS: f64 = 25.0;
const SENTENCE_SPLIT_SECS: f64 = 15.0;

/// Rebuild transcript segments: a new segment on speaker change, on a long silence,
/// after a sentence end once a segment is long, and never beyond 25 s.
pub fn build_segments(words: &[DiarWord]) -> Vec<DiarSegment> {
    let mut segments: Vec<DiarSegment> = Vec::new();
    let mut current: Option<DiarSegment> = None;
    let mut last_speaker: Option<String> = None;

    for w in words {
        let token = w.text.trim();
        if token.is_empty() {
            continue;
        }
        let speaker = w
            .speaker
            .clone()
            .or_else(|| last_speaker.clone())
            .unwrap_or_else(|| "speaker_0".to_string());
        last_speaker = Some(speaker.clone());

        if let Some(seg) = current.as_ref() {
            let sentence_done = seg.text.ends_with(['.', '?', '!', '…']) && seg.end - seg.start >= SENTENCE_SPLIT_SECS;
            if seg.speaker != speaker
                || w.start - seg.end > MAX_GAP_SECS
                || w.end - seg.start > MAX_SEGMENT_SECS
                || sentence_done
            {
                segments.push(current.take().unwrap());
            }
        }

        match current.as_mut() {
            Some(seg) => {
                if !is_attached_punctuation(token) {
                    seg.text.push(' ');
                }
                seg.text.push_str(token);
                seg.end = w.end.max(seg.end);
            }
            None => {
                current = Some(DiarSegment { speaker, start: w.start, end: w.end, text: token.to_string() });
            }
        }
    }
    if let Some(seg) = current {
        segments.push(seg);
    }
    segments
}

/// Tokens that must stick to the previous word ("," "." "..." ")"), French spacing
/// before ? ! : ; is kept since providers emit them as separate tokens with a space.
fn is_attached_punctuation(token: &str) -> bool {
    matches!(token, "," | "." | "…" | "..." | ")" | "»" | "-") && token.len() <= 3
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpeakerStats {
    pub seconds: f64,
    pub turns: usize,
}

pub fn speaker_stats(segments: &[DiarSegment]) -> BTreeMap<String, SpeakerStats> {
    let mut stats: BTreeMap<String, SpeakerStats> = BTreeMap::new();
    for s in segments {
        let entry = stats.entry(s.speaker.clone()).or_default();
        entry.seconds += (s.end - s.start).max(0.0);
        entry.turns += 1;
    }
    stats
}

/// The diarized speaker that is the user: speaks mostly through the microphone while
/// remote participants come through system audio. None for in-person meetings (no
/// usable system-audio side) or when nobody is clearly mic-dominant.
pub fn identify_self(segments: &[DiarSegment], timeline: &SourceTimeline) -> Option<String> {
    if timeline.mic_share()? > 0.95 {
        return None;
    }
    let mut per_speaker: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for s in segments {
        if let Some((mic, sys)) = timeline.energy(s.start, s.end) {
            let e = per_speaker.entry(&s.speaker).or_default();
            e.0 += mic;
            e.1 += sys;
        }
    }
    per_speaker
        .into_iter()
        .filter(|(_, (mic, sys))| mic / (mic + sys) > 0.7)
        .max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(speaker, _)| speaker.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, end: f64, spk: Option<&str>) -> DiarWord {
        DiarWord { text: text.into(), start, end, speaker: spk.map(String::from) }
    }

    #[test]
    fn builds_segments_on_speaker_change_and_gaps() {
        let words = vec![
            w("Bonjour", 0.0, 0.4, Some("speaker_0")),
            w("Marie.", 0.5, 0.9, Some("speaker_0")),
            w("Bonjour", 1.2, 1.6, Some("speaker_1")),
            w(",", 1.6, 1.6, None),
            w("Loïc", 1.7, 2.0, Some("speaker_1")),
            w("Alors", 5.0, 5.3, Some("speaker_1")),
        ];
        let segs = build_segments(&words);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "Bonjour Marie.");
        assert_eq!(segs[1].text, "Bonjour, Loïc");
        assert_eq!(segs[1].speaker, "speaker_1");
        assert_eq!(segs[2].start, 5.0);
    }

    #[test]
    fn caps_segment_length() {
        let words: Vec<DiarWord> = (0..80).map(|i| w("mot", i as f64 * 0.5, i as f64 * 0.5 + 0.4, Some("speaker_0"))).collect();
        let segs = build_segments(&words);
        assert!(segs.len() >= 2);
        assert!(segs.iter().all(|s| s.end - s.start <= MAX_SEGMENT_SECS));
    }

    #[test]
    fn merges_overlapping_parts_and_reconciles_speakers() {
        // Part A: 0-100 s; voices A0 then A1. Part B: 80-180 s, the provider swapped ids.
        let part_a = PartWords {
            offset: 0.0,
            duration: 100.0,
            words: vec![
                w("un", 10.0, 10.5, Some("speaker_0")),
                w("deux", 50.0, 50.5, Some("speaker_1")),
                w("trois", 85.0, 85.5, Some("speaker_0")),
                w("quatre", 95.0, 95.5, Some("speaker_1")),
            ],
        };
        let part_b = PartWords {
            offset: 80.0,
            duration: 100.0,
            words: vec![
                w("trois", 5.0, 5.5, Some("speaker_1")),   // 85 s, same voice as A speaker_0
                w("quatre", 15.0, 15.5, Some("speaker_0")), // 95 s, same voice as A speaker_1
                w("cinq", 60.0, 60.5, Some("speaker_1")),   // 140 s, A speaker_0
                w("six", 70.0, 70.5, Some("speaker_2")),    // 150 s, a new voice
            ],
        };
        let merged = merge_parts(vec![part_b, part_a]);
        let texts: Vec<&str> = merged.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["un", "deux", "trois", "quatre", "cinq", "six"]);
        let spk: Vec<&str> = merged.iter().map(|w| w.speaker.as_deref().unwrap()).collect();
        assert_eq!(spk, vec!["speaker_0", "speaker_1", "speaker_0", "speaker_1", "speaker_0", "speaker_2"]);
    }

    #[test]
    fn identifies_self_from_mic_energy() {
        let timeline = SourceTimeline {
            window_secs: 1.0,
            windows: vec![(0.05, 0.0001), (0.05, 0.0001), (0.0001, 0.05), (0.0001, 0.05)],
        };
        let segs = vec![
            DiarSegment { speaker: "speaker_0".into(), start: 0.0, end: 2.0, text: "a".into() },
            DiarSegment { speaker: "speaker_1".into(), start: 2.0, end: 4.0, text: "b".into() },
        ];
        assert_eq!(identify_self(&segs, &timeline).as_deref(), Some("speaker_0"));

        let in_person = SourceTimeline { window_secs: 1.0, windows: vec![(0.05, 0.0); 4] };
        assert_eq!(identify_self(&segs, &in_person), None);
    }

    #[test]
    fn stats_sum_time_and_turns() {
        let segs = vec![
            DiarSegment { speaker: "speaker_0".into(), start: 0.0, end: 2.0, text: "a".into() },
            DiarSegment { speaker: "speaker_0".into(), start: 3.0, end: 4.0, text: "b".into() },
        ];
        let stats = speaker_stats(&segs);
        assert_eq!(stats["speaker_0"], SpeakerStats { seconds: 3.0, turns: 2 });
    }
}

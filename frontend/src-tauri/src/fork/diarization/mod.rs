// fork/diarization/mod.rs
//
// End-of-meeting speaker identification. The recorded audio is re-encoded to small
// opus parts, transcribed with word-level diarization through OpenRouter
// (elevenlabs/scribe-v2), and the result REPLACES the live transcript: it has full
// context, better punctuation and one speaker per segment, while live chunks were cut
// from mixed audio. The live version is kept in `transcripts.live.json` and can be
// restored. Speakers are then named from the participant list by an LLM.

pub mod align;
pub mod naming;

use self::align::{DiarSegment, DiarWord, PartWords};
use super::meeting_context::MeetingContextRepository;
use super::openrouter_stt::{self, SttOptions};
use super::preferences::{self as prefs, PreferencesRepository};
use super::speakers::{MeetingSpeaker, SpeakersRepository};
use crate::state::AppState;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Up to this length the whole meeting goes in one request (a 36 min file took 32 s,
/// under the ~60 s upstream limit).
const SINGLE_REQUEST_MAX_SECS: f64 = 25.0 * 60.0;
const PART_SECS: f64 = 20.0 * 60.0;
const OVERLAP_SECS: f64 = 20.0;
const PARALLEL_PARTS: usize = 3;
const LIVE_BACKUP_FILE: &str = "transcripts.live.json";

static IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static CANCELLED: AtomicBool = AtomicBool::new(false);

struct Guard;

impl Guard {
    fn acquire() -> Result<Self, String> {
        IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Guard)
            .map_err(|_| "Une identification des intervenants est déjà en cours".to_string())
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationResult {
    pub meeting_id: String,
    pub segments: usize,
    pub speakers: usize,
    pub named: usize,
    pub cost_usd: f64,
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, meeting_id: &str, stage: &str, progress: u32, message: &str) {
    let _ = app.emit(
        "diarization-progress",
        serde_json::json!({ "meetingId": meeting_id, "stage": stage, "progress": progress, "message": message }),
    );
}

fn check_cancelled() -> Result<(), String> {
    if CANCELLED.load(Ordering::SeqCst) {
        Err("Identification des intervenants annulée".to_string())
    } else {
        Ok(())
    }
}

pub async fn meeting_folder(pool: &SqlitePool, meeting_id: &str) -> Result<PathBuf, String> {
    let folder: Option<(Option<String>,)> = sqlx::query_as("SELECT folder_path FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    folder
        .and_then(|(f,)| f)
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .ok_or_else(|| "Dossier d'enregistrement introuvable pour cette réunion".to_string())
}

fn ffmpeg_command() -> Result<std::process::Command, String> {
    let path = crate::audio::ffmpeg::find_ffmpeg_path().ok_or("ffmpeg introuvable")?;
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(command)
}

/// Duration of a media file, parsed from `ffmpeg -i` ("Duration: 00:36:38.40").
pub fn audio_duration(audio: &Path) -> Result<f64, String> {
    probe_duration(audio)
}

fn probe_duration(audio: &Path) -> Result<f64, String> {
    let output = ffmpeg_command()?
        .args(["-hide_banner", "-i"])
        .arg(audio)
        .output()
        .map_err(|e| format!("ffmpeg: {}", e))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_ffmpeg_duration(&stderr).ok_or_else(|| "Durée de l'audio illisible".to_string())
}

pub(crate) fn parse_ffmpeg_duration(stderr: &str) -> Option<f64> {
    let rest = stderr.split("Duration: ").nth(1)?;
    let stamp = rest.split(',').next()?.trim();
    let mut parts = stamp.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}

/// (offset, length) of each request; consecutive parts overlap by OVERLAP_SECS.
pub(crate) fn plan_parts(duration: f64) -> Vec<(f64, f64)> {
    if duration <= SINGLE_REQUEST_MAX_SECS {
        return vec![(0.0, duration)];
    }
    let mut parts = Vec::new();
    let mut start = 0.0;
    while start < duration {
        let offset = if start == 0.0 { 0.0 } else { start - OVERLAP_SECS };
        let end = (start + PART_SECS).min(duration);
        parts.push((offset, end - offset));
        start += PART_SECS;
    }
    // Fold a tiny last part into the previous one
    if parts.len() > 1 && parts.last().map(|p| p.1 < 120.0).unwrap_or(false) {
        let last = parts.pop().unwrap();
        if let Some(prev) = parts.last_mut() {
            prev.1 = last.0 + last.1 - prev.0;
        }
    }
    parts
}

/// Encode [offset, offset+len) to mono 16 kHz opus 24 kb/s (~11 MB/h), mp3 if opus fails.
fn encode_part(audio: &Path, dir: &Path, index: usize, offset: f64, len: f64) -> Result<(PathBuf, &'static str), String> {
    for (codec, ext, extra) in [("libopus", "ogg", ["-b:a", "24k", "-application", "voip"]), ("libmp3lame", "mp3", ["-b:a", "32k", "-q:a", "9"])] {
        let out = dir.join(format!("part_{:03}.{}", index, ext));
        let status = ffmpeg_command()?
            .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", &format!("{:.3}", offset), "-t", &format!("{:.3}", len), "-i"])
            .arg(audio)
            .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", codec])
            .args(extra)
            .arg(&out)
            .status()
            .map_err(|e| format!("ffmpeg: {}", e))?;
        if status.success() && out.exists() {
            return Ok((out, ext));
        }
        warn!("ffmpeg could not encode part {} with {}", index, codec);
    }
    Err("Impossible de préparer l'audio pour l'identification des intervenants".to_string())
}

fn words_from_response(response: openrouter_stt::SttResponse) -> Vec<DiarWord> {
    if !response.words.is_empty() {
        return response
            .words
            .iter()
            .map(|w| DiarWord { text: w.word.clone(), start: w.start, end: w.end, speaker: w.speaker_key() })
            .collect();
    }
    // Providers that only diarize segments: one pseudo-word per segment
    response
        .segments
        .iter()
        .map(|s| DiarWord {
            text: s.text.clone(),
            start: s.start,
            end: s.end,
            speaker: openrouter_stt::speaker_key(s.speaker.as_ref()),
        })
        .collect()
}

async fn load_transcripts(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<crate::api::TranscriptSegment>, String> {
    let rows = sqlx::query_as::<_, crate::database::models::Transcript>(
        "SELECT * FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|t| crate::api::TranscriptSegment {
            id: t.id,
            text: t.transcript,
            timestamp: t.timestamp,
            audio_start_time: t.audio_start_time,
            audio_end_time: t.audio_end_time,
            duration: t.duration,
            speaker: t.speaker,
        })
        .collect())
}

async fn replace_transcripts(
    pool: &SqlitePool,
    folder: &Path,
    meeting_id: &str,
    segments: &[crate::api::TranscriptSegment],
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for s in segments {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&s.id)
        .bind(meeting_id)
        .bind(&s.text)
        .bind(&s.timestamp)
        .bind(s.audio_start_time)
        .bind(s.audio_end_time)
        .bind(s.duration)
        .bind(&s.speaker)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    if let Err(e) = crate::audio::common::write_transcripts_json(folder, segments) {
        warn!("Failed to write transcripts.json: {}", e);
    }
    Ok(())
}

fn to_transcript_segments(segments: &[DiarSegment]) -> Vec<crate::api::TranscriptSegment> {
    let now = chrono::Utc::now().to_rfc3339();
    segments
        .iter()
        .map(|s| crate::api::TranscriptSegment {
            id: format!("transcript-{}", uuid::Uuid::new_v4()),
            text: s.text.clone(),
            timestamp: now.clone(),
            audio_start_time: Some(s.start),
            audio_end_time: Some(s.end),
            duration: Some((s.end - s.start).max(0.0)),
            speaker: Some(s.speaker.clone()),
        })
        .collect()
}

pub async fn diarize_meeting<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) -> Result<DiarizationResult, String> {
    let _guard = Guard::acquire()?;
    CANCELLED.store(false, Ordering::SeqCst);
    let pool = app.state::<AppState>().db_manager.pool().clone();

    emit_progress(app, meeting_id, "preparing", 5, "Préparation de l'audio...");
    let folder = meeting_folder(&pool, meeting_id).await?;
    let audio = crate::audio::retranscription::find_audio_file(&folder).map_err(|e| e.to_string())?;
    let api_key = super::openrouter_chat::api_key(app).await?;
    let model = PreferencesRepository::get(&pool, prefs::DIARIZATION_MODEL).await;
    let language = Some(PreferencesRepository::get(&pool, prefs::STT_LANGUAGE).await).filter(|l| !l.is_empty() && l != "auto");
    let zdr = PreferencesRepository::get(&pool, prefs::PRIVACY_ZDR).await == "true";
    let context = MeetingContextRepository::get(&pool, meeting_id).await.map_err(|e| e.to_string())?;

    let duration = probe_duration(&audio)?;
    if duration < 5.0 {
        return Err("Enregistrement trop court pour identifier les intervenants".to_string());
    }
    let plan = plan_parts(duration);
    info!("🗣️ Diarization of {} ({:.0}s) in {} part(s) with {}", meeting_id, duration, plan.len(), model);

    // Keep the live transcript once, before any paid call, so the user can come back to it
    let backup = folder.join(LIVE_BACKUP_FILE);
    if !backup.exists() {
        let live = load_transcripts(&pool, meeting_id).await?;
        if !live.is_empty() {
            let json = serde_json::to_string_pretty(&live).map_err(|e| e.to_string())?;
            std::fs::write(&backup, json).map_err(|e| format!("Sauvegarde du direct impossible : {}", e))?;
        }
    }

    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut encoded = Vec::new();
    for (index, &(offset, len)) in plan.iter().enumerate() {
        check_cancelled()?;
        let (path, ext) = encode_part(&audio, workdir.path(), index, offset, len)?;
        encoded.push((index, offset, len, path, ext));
    }

    emit_progress(app, meeting_id, "transcribing", 20, "Identification des intervenants (cloud)...");
    let client = openrouter_stt::build_client(Duration::from_secs(240));
    let opts = SttOptions {
        model: model.clone(),
        language,
        diarize: true,
        verbose: true,
        vocabulary: context.vocabulary(),
        zdr,
    };
    let total_parts = encoded.len();
    let slots = Arc::new(tokio::sync::Semaphore::new(PARALLEL_PARTS));
    let mut handles = Vec::new();
    for (index, offset, len, path, ext) in encoded {
        let (client, api_key, opts, slots) = (client.clone(), api_key.clone(), opts.clone(), slots.clone());
        handles.push(tokio::spawn(async move {
            let _slot = slots.acquire_owned().await.map_err(|e| e.to_string())?;
            check_cancelled()?;
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            info!("🗣️ Sending part {}/{} ({:.0}s, {} KB)", index + 1, total_parts, len, bytes.len() / 1024);
            let response = openrouter_stt::transcribe_with_retry(&client, &api_key, &bytes, ext, &opts)
                .await
                .map_err(|e| {
                    if e.is_unsupported_option() {
                        format!("Le modèle {} ne sait pas identifier les intervenants via OpenRouter", opts.model)
                    } else {
                        e.to_string()
                    }
                })?;
            Ok::<_, String>((offset, len, response))
        }));
    }

    let mut parts = Vec::new();
    let mut cost = 0.0;
    let mut audio_seconds = 0.0;
    let mut failure: Option<String> = None;
    for handle in handles {
        if failure.is_some() {
            // One part failed: the meeting cannot be rebuilt, stop paying for the others
            handle.abort();
            continue;
        }
        match handle.await.map_err(|e| e.to_string()).and_then(|r| r) {
            Ok((offset, len, response)) => {
                if let Some(usage) = response.usage.as_ref() {
                    cost += usage.cost.unwrap_or(0.0);
                    audio_seconds += usage.seconds.unwrap_or(len);
                }
                parts.push(PartWords { offset, duration: len, words: words_from_response(response) });
            }
            Err(e) => {
                CANCELLED.store(true, Ordering::SeqCst);
                failure = Some(e);
            }
        }
    }
    if cost > 0.0 {
        if let Err(e) = super::usage::record(&pool, meeting_id, "diarization", Some(&model), Some(audio_seconds), Some(cost)).await {
            warn!("Failed to record diarization usage: {}", e);
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    let words = align::merge_parts(parts);
    if !words.iter().any(|w| w.speaker.is_some()) {
        return Err(format!("Le modèle {} n'a renvoyé aucun intervenant", model));
    }
    let segments = align::build_segments(&words);
    if segments.is_empty() {
        return Err("Aucune parole détectée dans l'enregistrement".to_string());
    }

    // Who is the user? (mic-dominant speaker, video calls only)
    let self_speaker = if context.is_in_person() {
        None
    } else {
        super::source_timeline::load_from_folder(&folder).and_then(|t| align::identify_self(&segments, &t))
    };

    emit_progress(app, meeting_id, "naming", 75, "Attribution des noms...");
    let speaker_keys: HashSet<String> = segments.iter().map(|s| s.speaker.clone()).collect();
    let mut names = Vec::new();
    if !context.participants.is_empty() || segments.len() > 3 {
        let naming_model = PreferencesRepository::get(&pool, prefs::NAMING_MODEL).await;
        let messages = [
            super::openrouter_chat::ChatMessage::system(naming::SYSTEM_PROMPT),
            super::openrouter_chat::ChatMessage::user(vec![super::openrouter_chat::ContentPart::text(
                naming::build_user_prompt(&segments, &context, self_speaker.as_deref()),
            )]),
        ];
        match super::openrouter_chat::chat(&api_key, &naming_model, &messages, true, 1500, zdr).await {
            Ok(outcome) => {
                if let Some(json) = super::openrouter_chat::extract_json(&outcome.text) {
                    names = naming::parse_answer(&json, &context, &speaker_keys);
                }
                let _ = super::usage::record(&pool, meeting_id, "naming", Some(&naming_model), None, outcome.cost_usd).await;
            }
            Err(e) => warn!("Speaker naming failed (labels stay generic): {}", e),
        }
    }

    emit_progress(app, meeting_id, "saving", 90, "Enregistrement de la transcription...");
    replace_transcripts(&pool, &folder, meeting_id, &to_transcript_segments(&segments)).await?;

    let self_name = context.participants.iter().find(|p| p.is_self).map(|p| p.name.trim().to_string());
    let mut speaker_rows: Vec<MeetingSpeaker> = Vec::new();
    let mut keys: Vec<&String> = speaker_keys.iter().collect();
    keys.sort();
    for key in keys {
        let named = names.iter().find(|n| &n.speaker == key);
        let is_self = self_speaker.as_deref() == Some(key.as_str());
        speaker_rows.push(match named {
            Some(n) => MeetingSpeaker {
                speaker_key: key.clone(),
                display_name: Some(n.display_name.clone()),
                participant_id: n.participant_id.clone(),
                source: "auto".into(),
                confidence: Some(n.confidence),
            },
            None if is_self => MeetingSpeaker {
                speaker_key: key.clone(),
                display_name: self_name.clone(),
                participant_id: None,
                source: "mic-anchor".into(),
                confidence: None,
            },
            None => MeetingSpeaker {
                speaker_key: key.clone(),
                display_name: None,
                participant_id: None,
                source: "auto".into(),
                confidence: None,
            },
        });
    }
    SpeakersRepository::replace_all(&pool, meeting_id, &speaker_rows)
        .await
        .map_err(|e| e.to_string())?;

    let result = DiarizationResult {
        meeting_id: meeting_id.to_string(),
        segments: segments.len(),
        speakers: speaker_rows.len(),
        named: speaker_rows.iter().filter(|s| s.display_name.is_some()).count(),
        cost_usd: cost,
    };
    emit_progress(app, meeting_id, "complete", 100, "Intervenants identifiés");
    info!("🗣️ Diarization done: {:?}", result);
    Ok(result)
}

/// Restore the live transcript saved before the first diarization.
pub async fn revert<R: Runtime>(app: &AppHandle<R>, meeting_id: &str) -> Result<(), String> {
    let pool = app.state::<AppState>().db_manager.pool().clone();
    let folder = meeting_folder(&pool, meeting_id).await?;
    let raw = std::fs::read_to_string(folder.join(LIVE_BACKUP_FILE))
        .map_err(|_| "Aucune transcription du direct sauvegardée pour cette réunion".to_string())?;
    let live: Vec<crate::api::TranscriptSegment> = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    replace_transcripts(&pool, &folder, meeting_id, &live).await?;
    sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ? AND speaker_key LIKE 'speaker_%'")
        .bind(meeting_id)
        .execute(&pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn api_diarize_meeting<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<DiarizationResult, String> {
    let result = diarize_meeting(&app, &meeting_id).await;
    if let Err(ref e) = result {
        warn!("Diarization failed for {}: {}", meeting_id, e);
        let _ = app.emit("diarization-error", serde_json::json!({ "meetingId": meeting_id, "error": e }));
    }
    result
}

#[tauri::command]
pub async fn api_cancel_diarization() -> Result<(), String> {
    CANCELLED.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn api_is_diarization_in_progress() -> bool {
    IN_PROGRESS.load(Ordering::SeqCst)
}

#[tauri::command]
pub async fn api_revert_diarization<R: Runtime>(app: AppHandle<R>, meeting_id: String) -> Result<(), String> {
    revert(&app, &meeting_id).await
}

/// Whether a live transcript backup exists (the UI then offers "restore live version").
#[tauri::command]
pub async fn api_has_live_transcript_backup(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<bool, String> {
    Ok(meeting_folder(state.db_manager.pool(), &meeting_id)
        .await
        .map(|f| f.join(LIVE_BACKUP_FILE).exists())
        .unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ffmpeg_duration_line() {
        let stderr = "Input #0, mov,mp4, from 'audio.mp4':\n  Duration: 00:36:38.40, start: 0.000000, bitrate: 64 kb/s";
        assert!((parse_ffmpeg_duration(stderr).unwrap() - 2198.4).abs() < 1e-6);
        assert!(parse_ffmpeg_duration("no duration").is_none());
    }

    #[test]
    fn short_meetings_use_one_request() {
        assert_eq!(plan_parts(1800.0 - 400.0), vec![(0.0, 1400.0)]);
    }

    #[test]
    fn long_meetings_are_split_with_overlap() {
        let parts = plan_parts(3600.0);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], (0.0, 1200.0));
        assert_eq!(parts[1], (1180.0, 1220.0));
        assert_eq!(parts[2], (2380.0, 1220.0));
        // last part ends at the end of the meeting
        let last = parts.last().unwrap();
        assert!((last.0 + last.1 - 3600.0).abs() < 1e-6);
    }

    #[test]
    fn tiny_last_part_is_folded_into_previous() {
        let parts = plan_parts(2460.0); // 41 min: 20 + 20 + 1
        assert_eq!(parts.len(), 2);
        let last = parts.last().unwrap();
        assert!((last.0 + last.1 - 2460.0).abs() < 1e-6);
    }
}

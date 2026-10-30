// fork/resume.rs
//
// Resume a stopped meeting. The new session records into <meeting>/.reprise-N/ so the
// meeting's own files are never touched while it runs; it continues the meeting's
// timeline (transcript times and screenshots are shifted by the meeting's length).
// At stop it is glued onto the meeting: audio concatenated and checked, mic/system
// energy and sessions merged; the transcript is appended at save time.

use super::meeting_time::{read_metadata, sessions_from_metadata, Session};
use super::source_timeline::{self, SourceTimeline};
use crate::api::TranscriptSegment;
use crate::state::AppState;
use chrono::Utc;
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Runtime};

const SESSION_PREFIX: &str = ".reprise-";
const LIVE_BACKUP_FILE: &str = "transcripts.live.json";
/// Glued audio must last the sum of both parts within this margin
const MERGE_TOLERANCE_SECS: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeTarget {
    pub meeting_id: String,
    pub title: String,
    /// The meeting's folder
    pub folder: PathBuf,
    /// Where this session records (inside the meeting's folder)
    pub session_dir: PathBuf,
    /// Where this session starts on the meeting's timeline (seconds)
    pub offset_seconds: f64,
    pub session_number: u32,
}

/// Resume being recorded
static ACTIVE: Lazy<Mutex<Option<ResumeTarget>>> = Lazy::new(|| Mutex::new(None));
/// Resume stopped, waiting for the UI to save its transcript
static FINISHED: Lazy<Mutex<Option<ResumeTarget>>> = Lazy::new(|| Mutex::new(None));

fn lock(m: &'static Mutex<Option<ResumeTarget>>) -> MutexGuard<'static, Option<ResumeTarget>> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn active() -> Option<ResumeTarget> {
    lock(&ACTIVE).clone()
}

/// Folder the recording saver uses instead of creating a new meeting folder.
pub fn session_folder() -> Option<PathBuf> {
    lock(&ACTIVE).as_ref().map(|t| t.session_dir.clone())
}

/// Shift of the live timeline (0 outside a resume).
pub fn offset() -> f64 {
    lock(&ACTIVE).as_ref().map(|t| t.offset_seconds).unwrap_or(0.0)
}

/// Screenshots of a resumed session go to the meeting's own screens/.
pub fn capture_folder() -> Option<PathBuf> {
    lock(&ACTIVE).as_ref().map(|t| t.folder.clone())
}

/// Next free `.reprise-N`; session 1 is the original recording.
pub fn next_session_number(folder: &Path, recorded_sessions: usize) -> u32 {
    let mut n = recorded_sessions.max(1) as u32 + 1;
    while folder.join(format!("{SESSION_PREFIX}{n}")).exists() {
        n += 1;
    }
    n
}

/// A resumed session whose audio was never glued onto the meeting.
pub fn unmerged_session(folder: &Path) -> Option<PathBuf> {
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name().is_some_and(|n| n.to_string_lossy().starts_with(SESSION_PREFIX)) && path.join("audio.mp4").exists()
        })
}

/// Meeting energy windows, padded with silence (or cut) up to `offset_seconds`,
/// followed by the session's windows.
pub fn merge_timelines(mut base: SourceTimeline, session: SourceTimeline, offset_seconds: f64) -> SourceTimeline {
    let window = if base.window_secs > 0.0 { base.window_secs } else { session.window_secs };
    if window <= 0.0 || (session.window_secs > 0.0 && (session.window_secs - window).abs() > 1e-6) {
        warn!("Source energy windows differ ({} s / {} s): resumed session not merged", base.window_secs, session.window_secs);
        return base;
    }
    base.windows.resize((offset_seconds / window).round() as usize, (0.0, 0.0));
    base.windows.extend(session.windows);
    base.window_secs = window;
    base
}

/// Record a session in metadata.json: keep every other field, rebuild session 1 from
/// created_at/completed_at when the list does not exist yet.
pub fn add_session(meta: &mut Value, session: Session) {
    if !meta.is_object() {
        *meta = Value::Object(Default::default());
    }
    let mut sessions = sessions_from_metadata(meta);
    if let Some(end) = &session.ended_at {
        meta["completed_at"] = Value::String(end.clone());
    }
    sessions.push(session);
    let total: f64 = sessions.iter().filter_map(|s| s.duration_seconds).sum();
    if total > 0.0 {
        meta["duration_seconds"] = serde_json::json!(total);
    }
    meta["sessions"] = serde_json::to_value(&sessions).unwrap_or(Value::Null);
}

fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn concat_line(path: &Path) -> String {
    format!("file '{}'\n", path.to_string_lossy().replace('\'', "'\\''"))
}

/// Glue `part` after `main`. `main` is replaced only by a checked file (renamed over it).
pub fn concat_audio(main: &Path, part: &Path, workdir: &Path) -> Result<(), String> {
    let expected = super::diarization::audio_duration(main)? + super::diarization::audio_duration(part)?;
    let list = workdir.join("concat.txt");
    std::fs::write(&list, format!("{}{}", concat_line(main), concat_line(part))).map_err(|e| e.to_string())?;
    let tmp = main.with_file_name("audio.reprise.tmp.mp4");
    let attempts: [&[&str]; 2] = [&["-c", "copy"], &["-c:a", "aac", "-b:a", "128k"]];
    for codec in attempts {
        let _ = std::fs::remove_file(&tmp);
        let output = super::diarization::ffmpeg_command()?
            .args(["-hide_banner", "-y", "-f", "concat", "-safe", "0", "-i"])
            .arg(&list)
            .args(codec)
            .args(["-movflags", "+faststart"])
            .arg(&tmp)
            .output()
            .map_err(|e| format!("ffmpeg : {e}"))?;
        let glued = output.status.success().then(|| super::diarization::audio_duration(&tmp).ok()).flatten();
        match glued {
            Some(duration) if (duration - expected).abs() <= MERGE_TOLERANCE_SECS => {
                std::fs::rename(&tmp, main).map_err(|e| format!("Remplacement de audio.mp4 impossible : {e}"))?;
                return Ok(());
            }
            other => warn!("Audio glue with {:?} gave {:?} s instead of {:.1} s", codec, other, expected),
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err("Recollage de l'audio impossible : la reprise reste dans son dossier".to_string())
}

/// Add the resumed session's live segments to transcripts.live.json when it exists,
/// so "restore the live transcript" brings back both parts. Returns how many were added.
pub fn append_live_backup(folder: &Path, segments: &[TranscriptSegment]) -> Result<usize, String> {
    let path = folder.join(LIVE_BACKUP_FILE);
    if segments.is_empty() || !path.exists() {
        return Ok(0);
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut live: Vec<Value> = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    for segment in segments {
        live.push(serde_json::to_value(segment).map_err(|e| e.to_string())?);
    }
    write_json_atomic(&path, &Value::Array(live))?;
    Ok(segments.len())
}

/// Glue the stopped session onto its meeting. The session folder is removed only
/// when its audio is glued (or when there was no audio to glue).
fn glue_session(target: &ResumeTarget, ended_at: chrono::DateTime<Utc>) -> Result<(), String> {
    let main_audio = target.folder.join("audio.mp4");
    let part_audio = target.session_dir.join("audio.mp4");
    let audio = if part_audio.exists() && main_audio.exists() {
        concat_audio(&main_audio, &part_audio, &target.session_dir)
    } else if part_audio.exists() {
        Err("La réunion n'a pas d'audio.mp4 : l'audio de la reprise reste dans son dossier".to_string())
    } else {
        Ok(())
    };

    if let Some(session_energy) = source_timeline::load_from_folder(&target.session_dir) {
        let base = source_timeline::load_from_folder(&target.folder).unwrap_or_default();
        let merged = merge_timelines(base, session_energy, target.offset_seconds);
        if let Err(e) = source_timeline::write(&target.folder, &merged) {
            warn!("Could not merge source_energy.json: {}", e);
        }
    }

    let session_meta = read_metadata(&target.session_dir).unwrap_or(Value::Null);
    let duration = session_meta.get("duration_seconds").and_then(Value::as_f64);
    let started_at = session_meta
        .get("created_at")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| (ended_at - chrono::Duration::seconds(duration.unwrap_or(0.0) as i64)).to_rfc3339());
    let mut meta = read_metadata(&target.folder).unwrap_or(Value::Null);
    add_session(
        &mut meta,
        Session { started_at, ended_at: Some(ended_at.to_rfc3339()), offset_seconds: target.offset_seconds, duration_seconds: duration },
    );
    write_json_atomic(&target.folder.join("metadata.json"), &meta)?;

    audio?;
    std::fs::remove_dir_all(&target.session_dir).map_err(|e| format!("Nettoyage de {} : {e}", target.session_dir.display()))
}

/// Called by stop_recording once the session is saved. Returns the meeting folder
/// (reported to the UI instead of the session folder); None outside a resume.
pub fn finish_after_stop<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let target = lock(&ACTIVE).take()?;
    match glue_session(&target, Utc::now()) {
        Ok(()) => info!("⏯️ Session {} glued onto meeting {}", target.session_number, target.meeting_id),
        Err(message) => {
            warn!("Resume of {} not fully glued: {}", target.meeting_id, message);
            let _ = app.emit("resume-merge-failed", serde_json::json!({ "meetingId": target.meeting_id, "message": message }));
        }
    }
    let folder = target.folder.clone();
    *lock(&FINISHED) = Some(target);
    Some(folder)
}

/// The stopped resume whose folder is being saved, if any (consumed).
pub fn take_finished_for(folder_path: Option<&str>) -> Option<ResumeTarget> {
    let mut finished = lock(&FINISHED);
    let matches = finished
        .as_ref()
        .is_some_and(|t| folder_path.is_some_and(|f| Path::new(f) == t.folder.as_path()));
    if matches {
        finished.take()
    } else {
        None
    }
}

async fn insert_segments(pool: &SqlitePool, meeting_id: &str, segments: &[TranscriptSegment]) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for segment in segments {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("transcript-{}", uuid::Uuid::new_v4()))
        .bind(meeting_id)
        .bind(&segment.text)
        .bind(&segment.timestamp)
        .bind(segment.audio_start_time)
        .bind(segment.audio_end_time)
        .bind(segment.duration)
        .bind(&segment.speaker)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE meetings SET updated_at = ? WHERE id = ?")
        .bind(Utc::now())
        .bind(meeting_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Save a resumed session into its meeting (api_save_transcript).
pub async fn append_to_meeting(
    pool: &SqlitePool,
    target: &ResumeTarget,
    segments: &[TranscriptSegment],
    context: Option<&super::meeting_context::MeetingContext>,
) -> Result<(), String> {
    insert_segments(pool, &target.meeting_id, segments)
        .await
        .map_err(|e| format!("Ajout de la transcription à la réunion impossible : {e}"))?;
    if let Err(e) = append_live_backup(&target.folder, segments) {
        warn!("Could not extend {}: {}", LIVE_BACKUP_FILE, e);
    }
    super::persist_new_meeting_extras(pool, &target.meeting_id, context).await;
    super::notion::mark_dirty(&target.meeting_id);
    info!("⏯️ {} segment(s) appended to meeting {}", segments.len(), target.meeting_id);
    Ok(())
}

async fn summary_running(pool: &SqlitePool, meeting_id: &str) -> bool {
    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    status.is_some_and(|(s,)| matches!(s.to_lowercase().as_str(), "pending" | "processing"))
}

/// Length of the meeting's timeline: its audio, else the end of its last segment.
async fn timeline_end(pool: &SqlitePool, meeting_id: &str, folder: &Path) -> f64 {
    let audio = folder.join("audio.mp4");
    if audio.exists() {
        match super::diarization::audio_duration(&audio) {
            Ok(duration) => return duration,
            Err(e) => warn!("Could not measure {}: {}", audio.display(), e),
        }
    }
    let last: Option<(Option<f64>,)> = sqlx::query_as("SELECT MAX(audio_end_time) FROM transcripts WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    last.and_then(|(end,)| end).unwrap_or(0.0)
}

async fn check(pool: &SqlitePool, meeting_id: &str) -> Result<PathBuf, String> {
    if crate::audio::recording_commands::is_recording().await {
        return Err("Un enregistrement est déjà en cours".to_string());
    }
    if super::diarization::is_running() {
        return Err("Identification des intervenants en cours : attendez la fin pour reprendre".to_string());
    }
    if summary_running(pool, meeting_id).await {
        return Err("Compte rendu en cours de génération : attendez la fin pour reprendre".to_string());
    }
    let folder = super::diarization::meeting_folder(pool, meeting_id).await?;
    if let Some(dir) = unmerged_session(&folder) {
        return Err(format!("Une reprise précédente n'a pas été recollée ({})", dir.display()));
    }
    Ok(folder)
}

#[tauri::command]
pub async fn api_check_resume(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<(), String> {
    check(state.db_manager.pool(), &meeting_id).await.map(|_| ())
}

/// Arm a resume; the next recording start records into the meeting.
#[tauri::command]
pub async fn api_prepare_resume(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<ResumeTarget, String> {
    let pool = state.db_manager.pool();
    let folder = check(pool, &meeting_id).await?;
    let (title,): (String,) = sqlx::query_as("SELECT title FROM meetings WHERE id = ?")
        .bind(&meeting_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    let recorded = read_metadata(&folder).map(|m| sessions_from_metadata(&m).len()).unwrap_or(1);
    let session_number = next_session_number(&folder, recorded);
    let target = ResumeTarget {
        offset_seconds: timeline_end(pool, &meeting_id, &folder).await,
        session_dir: folder.join(format!("{SESSION_PREFIX}{session_number}")),
        folder,
        meeting_id,
        title,
        session_number,
    };
    info!("⏯️ Resume armed: meeting {} from {:.1} s, session {}", target.meeting_id, target.offset_seconds, session_number);
    *lock(&FINISHED) = None;
    *lock(&ACTIVE) = Some(target.clone());
    Ok(target)
}

/// Drop an armed resume whose recording did not start (dialog cancelled, start failed).
#[tauri::command]
pub async fn api_cancel_resume() -> Result<(), String> {
    if crate::audio::recording_commands::is_recording().await {
        return Ok(());
    }
    if let Some(target) = lock(&ACTIVE).take() {
        if target.session_dir.exists() && !target.session_dir.join("audio.mp4").exists() {
            let _ = std::fs::remove_dir_all(&target.session_dir);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn api_active_resume() -> Option<ResumeTarget> {
    active()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tone(dir: &Path, name: &str, seconds: u32) -> Option<PathBuf> {
        crate::audio::ffmpeg::find_ffmpeg_path()?;
        let path = dir.join(name);
        let status = super::super::diarization::ffmpeg_command()
            .ok()?
            .args(["-hide_banner", "-y", "-f", "lavfi", "-i"])
            .arg(format!("sine=frequency=440:duration={seconds}"))
            .args(["-c:a", "aac", "-ar", "48000"])
            .arg(&path)
            .status()
            .ok()?;
        status.success().then_some(path)
    }

    fn segment(start: f64) -> TranscriptSegment {
        TranscriptSegment {
            id: "s".into(),
            text: "suite".into(),
            timestamp: "16:02:00".into(),
            audio_start_time: Some(start),
            audio_end_time: Some(start + 2.0),
            duration: Some(2.0),
            speaker: Some("mic".into()),
        }
    }

    #[test]
    fn session_numbers_skip_leftover_folders() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(next_session_number(dir.path(), 1), 2);
        std::fs::create_dir(dir.path().join(".reprise-2")).unwrap();
        assert_eq!(next_session_number(dir.path(), 1), 3);
        assert_eq!(next_session_number(dir.path(), 3), 4);
    }

    #[test]
    fn energy_is_padded_or_cut_to_the_offset() {
        let base = SourceTimeline { window_secs: 0.5, windows: vec![(1.0, 0.0); 3] };
        let session = SourceTimeline { window_secs: 0.5, windows: vec![(0.0, 1.0); 2] };
        let padded = merge_timelines(base.clone(), session.clone(), 2.5);
        assert_eq!(padded.windows.len(), 7);
        assert_eq!(padded.windows[3], (0.0, 0.0));
        assert_eq!(padded.windows[5], (0.0, 1.0));
        let cut = merge_timelines(base.clone(), session.clone(), 1.0);
        assert_eq!(cut.windows, vec![(1.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.0, 1.0)]);
        let other_window = SourceTimeline { window_secs: 0.6, windows: vec![(0.0, 1.0)] };
        assert_eq!(merge_timelines(base.clone(), other_window, 2.5).windows.len(), 3, "not merged");
    }

    #[test]
    fn sessions_are_added_and_other_fields_kept() {
        let mut meta = json!({
            "created_at": "2026-10-30T13:05:00+00:00",
            "completed_at": "2026-10-30T13:40:00+00:00",
            "duration_seconds": 2112.0,
            "summary_language": "fr",
        });
        add_session(&mut meta, Session {
            started_at: "2026-10-30T15:02:00+00:00".into(),
            ended_at: Some("2026-10-30T15:30:00+00:00".into()),
            offset_seconds: 2112.0,
            duration_seconds: Some(1680.0),
        });
        assert_eq!(meta["summary_language"], "fr");
        assert_eq!(meta["created_at"], "2026-10-30T13:05:00+00:00");
        assert_eq!(meta["completed_at"], "2026-10-30T15:30:00+00:00");
        assert_eq!(meta["duration_seconds"], 3792.0);
        let sessions = sessions_from_metadata(&meta);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].ended_at.as_deref(), Some("2026-10-30T13:40:00+00:00"));
        assert_eq!(sessions[1].offset_seconds, 2112.0);
    }

    #[test]
    fn live_backup_grows_only_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(append_live_backup(dir.path(), &[segment(40.0)]).unwrap(), 0);
        assert!(!dir.path().join(LIVE_BACKUP_FILE).exists());
        std::fs::write(dir.path().join(LIVE_BACKUP_FILE), "[{\"id\":\"a\",\"text\":\"début\",\"timestamp\":\"x\"}]").unwrap();
        assert_eq!(append_live_backup(dir.path(), &[segment(40.0), segment(42.0)]).unwrap(), 2);
        let live: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(dir.path().join(LIVE_BACKUP_FILE)).unwrap()).unwrap();
        assert_eq!(live.len(), 3);
        assert_eq!(live[2]["audio_start_time"], 42.0);
    }

    #[test]
    fn a_resumed_session_is_glued_onto_its_meeting() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_path_buf();
        let session_dir = folder.join(".reprise-2");
        std::fs::create_dir(&session_dir).unwrap();
        let (Some(_), Some(_)) = (tone(&folder, "audio.mp4", 3), tone(&session_dir, "audio.mp4", 2)) else {
            eprintln!("ffmpeg absent: glue test skipped");
            return;
        };
        std::fs::write(folder.join("metadata.json"), json!({ "created_at": "2026-10-30T13:05:00+00:00", "duration_seconds": 3.0 }).to_string()).unwrap();
        std::fs::write(session_dir.join("metadata.json"), json!({ "created_at": "2026-10-30T15:02:00+00:00", "duration_seconds": 2.0 }).to_string()).unwrap();
        source_timeline::write(&folder, &SourceTimeline { window_secs: 0.5, windows: vec![(1.0, 0.0); 6] }).unwrap();
        source_timeline::write(&session_dir, &SourceTimeline { window_secs: 0.5, windows: vec![(0.0, 1.0); 4] }).unwrap();
        let target = ResumeTarget {
            meeting_id: "m".into(),
            title: "R".into(),
            folder: folder.clone(),
            session_dir: session_dir.clone(),
            offset_seconds: 3.0,
            session_number: 2,
        };

        glue_session(&target, Utc::now()).unwrap();

        let glued = super::super::diarization::audio_duration(&folder.join("audio.mp4")).unwrap();
        assert!((glued - 5.0).abs() < 0.5, "glued audio lasts {glued} s");
        assert!(!session_dir.exists(), "session folder removed after a successful glue");
        assert!(unmerged_session(&folder).is_none());
        assert_eq!(source_timeline::load_from_folder(&folder).unwrap().windows.len(), 10);
        let meta = read_metadata(&folder).unwrap();
        assert_eq!(sessions_from_metadata(&meta).len(), 2);
    }

    #[test]
    fn a_failed_glue_keeps_the_meeting_audio_and_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_path_buf();
        let session_dir = folder.join(".reprise-2");
        std::fs::create_dir(&session_dir).unwrap();
        let Some(main) = tone(&folder, "audio.mp4", 3) else {
            eprintln!("ffmpeg absent: test skipped");
            return;
        };
        std::fs::write(session_dir.join("audio.mp4"), b"not audio").unwrap();
        let before = std::fs::read(&main).unwrap();
        let target = ResumeTarget {
            meeting_id: "m".into(),
            title: "R".into(),
            folder: folder.clone(),
            session_dir: session_dir.clone(),
            offset_seconds: 3.0,
            session_number: 2,
        };

        assert!(glue_session(&target, Utc::now()).is_err());
        assert_eq!(std::fs::read(&main).unwrap(), before, "meeting audio untouched");
        assert_eq!(unmerged_session(&folder), Some(session_dir));
    }

    #[test]
    fn finished_resume_matches_its_folder_only() {
        let target = ResumeTarget {
            meeting_id: "m".into(),
            title: "R".into(),
            folder: PathBuf::from("C:/x/Reunion"),
            session_dir: PathBuf::from("C:/x/Reunion/.reprise-2"),
            offset_seconds: 3.0,
            session_number: 2,
        };
        *lock(&FINISHED) = Some(target.clone());
        assert!(take_finished_for(Some("C:/x/Autre")).is_none());
        assert!(take_finished_for(None).is_none());
        assert_eq!(take_finished_for(Some("C:/x/Reunion")), Some(target));
        assert!(take_finished_for(Some("C:/x/Reunion")).is_none(), "consumed");
    }
}

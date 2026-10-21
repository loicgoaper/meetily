// fork/speakers.rs
//
// Display names of the speakers of a meeting. `transcripts.speaker` keeps the raw key
// ("mic", "system", "speaker_0"...); the human name lives in `meeting_speakers`, so a
// rename touches one row and manual renames always win over automatic naming.

use crate::state::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSpeaker {
    pub speaker_key: String,
    pub display_name: Option<String>,
    pub participant_id: Option<String>,
    /// 'auto' | 'manual' | 'mic-anchor'
    pub source: String,
    pub confidence: Option<f64>,
}

pub struct SpeakersRepository;

impl SpeakersRepository {
    pub async fn list(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<MeetingSpeaker>, sqlx::Error> {
        sqlx::query_as::<_, MeetingSpeaker>(
            "SELECT speaker_key, display_name, participant_id, source, confidence
             FROM meeting_speakers WHERE meeting_id = ? ORDER BY speaker_key",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    /// Replace all speakers of a meeting (after a diarization run).
    pub async fn replace_all(pool: &SqlitePool, meeting_id: &str, speakers: &[MeetingSpeaker]) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ?")
            .bind(meeting_id)
            .execute(&mut *tx)
            .await?;
        for s in speakers {
            sqlx::query(
                "INSERT INTO meeting_speakers (meeting_id, speaker_key, display_name, participant_id, source, confidence, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(meeting_id)
            .bind(&s.speaker_key)
            .bind(&s.display_name)
            .bind(&s.participant_id)
            .bind(&s.source)
            .bind(s.confidence)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    pub async fn rename(pool: &SqlitePool, meeting_id: &str, speaker_key: &str, display_name: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO meeting_speakers (meeting_id, speaker_key, display_name, source, updated_at)
             VALUES (?, ?, ?, 'manual', ?)
             ON CONFLICT(meeting_id, speaker_key) DO UPDATE SET
                display_name = excluded.display_name,
                source = 'manual',
                participant_id = NULL,
                confidence = NULL,
                updated_at = excluded.updated_at",
        )
        .bind(meeting_id)
        .bind(speaker_key)
        .bind(display_name.map(str::trim).filter(|n| !n.is_empty()))
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Every segment of `from_key` now belongs to `into_key` ("speaker 4 is speaker 1").
    pub async fn merge(pool: &SqlitePool, meeting_id: &str, from_key: &str, into_key: &str) -> Result<u64, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let moved = sqlx::query("UPDATE transcripts SET speaker = ? WHERE meeting_id = ? AND speaker = ?")
            .bind(into_key)
            .bind(meeting_id)
            .bind(from_key)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ? AND speaker_key = ?")
            .bind(meeting_id)
            .bind(from_key)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(moved)
    }
}

#[tauri::command]
pub async fn api_get_meeting_speakers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<MeetingSpeaker>, String> {
    SpeakersRepository::list(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| e.to_string())
}

/// Give a speaker a name for this meeting (empty name restores the default label).
#[tauri::command]
pub async fn api_rename_speaker(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    speaker_key: String,
    display_name: Option<String>,
) -> Result<(), String> {
    SpeakersRepository::rename(state.db_manager.pool(), &meeting_id, &speaker_key, display_name.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    super::notion::mark_dirty(&meeting_id);
    Ok(())
}

/// Merge two speakers of a meeting: all segments of `from_key` move to `into_key`.
/// Returns how many segments moved.
#[tauri::command]
pub async fn api_merge_speakers(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    from_key: String,
    into_key: String,
) -> Result<u64, String> {
    if from_key == into_key {
        return Ok(0);
    }
    let moved = SpeakersRepository::merge(state.db_manager.pool(), &meeting_id, &from_key, &into_key)
        .await
        .map_err(|e| e.to_string())?;
    super::notion::mark_dirty(&meeting_id);
    Ok(moved)
}

/// Reassign one transcript segment to another speaker key.
#[tauri::command]
pub async fn api_set_segment_speaker(
    state: tauri::State<'_, AppState>,
    transcript_id: String,
    speaker_key: Option<String>,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
        .bind(speaker_key)
        .bind(&transcript_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    if let Ok(Some((meeting_id,))) = sqlx::query_as::<_, (String,)>("SELECT meeting_id FROM transcripts WHERE id = ?")
        .bind(&transcript_id)
        .fetch_optional(pool)
        .await
    {
        super::notion::mark_dirty(&meeting_id);
    }
    Ok(())
}

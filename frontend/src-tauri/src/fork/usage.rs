// fork/usage.rs
//
// Cloud cost tracking: live STT usage is accumulated in memory during the recording
// and persisted with the meeting; batch steps (diarization, vision, naming) record
// their usage directly.

use crate::state::AppState;
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveUsage {
    pub model: Option<String>,
    pub audio_seconds: f64,
    pub cost_usd: f64,
    pub requests: u64,
    pub fallback_chunks: u64,
}

static LIVE_USAGE: Lazy<Mutex<LiveUsage>> = Lazy::new(|| Mutex::new(LiveUsage::default()));

pub fn reset_live(model: Option<String>) {
    if let Ok(mut u) = LIVE_USAGE.lock() {
        *u = LiveUsage { model, ..Default::default() };
    }
}

pub fn add_live(seconds: f64, cost: f64) -> LiveUsage {
    let mut u = LIVE_USAGE.lock().unwrap_or_else(|e| e.into_inner());
    u.audio_seconds += seconds;
    u.cost_usd += cost;
    u.requests += 1;
    u.clone()
}

pub fn add_live_fallback() {
    if let Ok(mut u) = LIVE_USAGE.lock() {
        u.fallback_chunks += 1;
    }
}

/// Take the live usage of the recording that just ended (resets the counter).
pub fn take_live() -> LiveUsage {
    let mut u = LIVE_USAGE.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *u)
}

pub async fn record(
    pool: &SqlitePool,
    meeting_id: &str,
    kind: &str,
    model: Option<&str>,
    audio_seconds: Option<f64>,
    cost_usd: Option<f64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO meeting_usage (id, meeting_id, kind, model, audio_seconds, cost_usd, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(format!("usage-{}", Uuid::new_v4()))
    .bind(meeting_id)
    .bind(kind)
    .bind(model)
    .bind(audio_seconds)
    .bind(cost_usd)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    pub kind: String,
    pub model: Option<String>,
    pub audio_seconds: Option<f64>,
    pub cost_usd: Option<f64>,
    pub created_at: String,
}

#[tauri::command]
pub async fn api_get_meeting_usage(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<UsageRow>, String> {
    sqlx::query_as::<_, UsageRow>(
        "SELECT kind, model, audio_seconds, cost_usd, created_at FROM meeting_usage
         WHERE meeting_id = ? ORDER BY created_at ASC",
    )
    .bind(meeting_id)
    .fetch_all(state.db_manager.pool())
    .await
    .map_err(|e| e.to_string())
}

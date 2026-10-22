// fork/retention.rs
//
// Screenshot lifecycle: captures not used in the summary are deleted after a delay
// (default 6 months, setting "screens.retention_days", "0" = never); captures shown in
// the summary are kept for good. Only meetings whose summary had a chance to use its
// captures (an illustration pass ran: screens/illustration.json) are purged, and only
// Meetily's own capture files listed in screens/index.json are ever deleted. The vision
// notes (text) are kept.

use super::illustrate::{referenced_files, MARKER_FILE};
use super::preferences::{self as prefs, PreferencesRepository};
use super::screen_capture::{is_capture_name, read_index, write_index, INDEX_LOCK, SCREENS_DIR};
use crate::state::AppState;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::{info, warn};
use serde::Serialize;
use sqlx::SqlitePool;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

pub const REPORT_FILE: &str = "retention.json";
const FIRST_RUN_DELAY: Duration = Duration::from_secs(120);
const RUN_EVERY: Duration = Duration::from_secs(24 * 3600);

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingPurge {
    pub meeting_id: String,
    pub title: String,
    pub created_at: String,
    pub files: usize,
    pub bytes: u64,
    pub kept: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionReport {
    pub days: u32,
    pub dry_run: bool,
    pub meetings: Vec<MeetingPurge>,
    pub files: usize,
    pub bytes: u64,
}

struct Target {
    meeting_id: String,
    title: String,
    created_at: String,
    screens: PathBuf,
}

pub async fn retention_days(pool: &SqlitePool) -> u32 {
    PreferencesRepository::get(pool, prefs::SCREENS_RETENTION_DAYS).await.trim().parse::<u32>().unwrap_or(180)
}

/// Meetings old enough, with a completed summary and an illustration pass.
async fn targets(pool: &SqlitePool, days: u32, now: DateTime<Utc>) -> Result<Vec<Target>, String> {
    if days == 0 {
        return Ok(Vec::new());
    }
    let cutoff = now - ChronoDuration::days(days as i64);
    let rows: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT m.id, m.title, m.created_at, m.folder_path, sp.status
         FROM meetings m LEFT JOIN summary_processes sp ON sp.meeting_id = m.id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|(meeting_id, title, created_at, folder, status)| {
            let created = DateTime::parse_from_rfc3339(&created_at).ok()?.with_timezone(&Utc);
            if created > cutoff || status.as_deref() != Some("completed") {
                return None;
            }
            let screens = PathBuf::from(folder?).join(SCREENS_DIR);
            screens.join(MARKER_FILE).is_file().then_some(Target { meeting_id, title, created_at, screens })
        })
        .collect())
}

/// Files of the summary (and its backup before a regeneration) that must stay.
async fn kept_files(pool: &SqlitePool, meeting_id: &str) -> Result<BTreeSet<String>, String> {
    let row: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT result, result_backup FROM summary_processes WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let (result, backup) = row.unwrap_or_default();
    let mut kept = referenced_files(result.as_deref().unwrap_or_default());
    kept.extend(referenced_files(backup.as_deref().unwrap_or_default()));
    Ok(kept)
}

/// Captures of a screens folder that may be deleted: listed in the index, not purged yet,
/// not used, named like a Meetily capture, a regular file directly inside the folder.
pub fn deletable(screens: &Path, kept: &BTreeSet<String>) -> Vec<(String, u64)> {
    read_index(screens)
        .into_iter()
        .filter(|f| !f.purged && !kept.contains(&f.file) && is_capture_name(&f.file))
        .filter_map(|f| {
            let path = screens.join(&f.file);
            if path.parent() != Some(screens) {
                return None;
            }
            match std::fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() => Some((f.file, meta.len())),
                // Already gone (deleted by hand): only marked as purged
                Err(_) => Some((f.file, 0)),
                Ok(_) => None,
            }
        })
        .collect()
}

/// Delete the files and mark them purged in the index (atomic write, under lock).
fn purge(screens: &Path, files: &[(String, u64)]) -> (usize, u64) {
    let _guard = INDEX_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut removed = Vec::new();
    let mut bytes = 0;
    for (file, size) in files {
        let path = screens.join(file);
        match std::fs::remove_file(&path) {
            Ok(()) => {
                removed.push(file.clone());
                bytes += size;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => removed.push(file.clone()),
            Err(e) => warn!("Could not delete {}: {}", path.display(), e),
        }
    }
    let mut index = read_index(screens);
    for frame in index.iter_mut().filter(|f| removed.contains(&f.file)) {
        frame.purged = true;
    }
    if let Err(e) = write_index(screens, &index) {
        warn!("Could not update {}: {}", screens.display(), e);
    }
    (removed.len(), bytes)
}

/// Preview (dry run) or apply the retention.
pub async fn run(pool: &SqlitePool, days: u32, dry_run: bool) -> Result<RetentionReport, String> {
    let mut report = RetentionReport { days, dry_run, ..Default::default() };
    for target in targets(pool, days, Utc::now()).await? {
        // Read just before deleting: a summary edited meanwhile keeps its images
        let kept = kept_files(pool, &target.meeting_id).await?;
        let files = deletable(&target.screens, &kept);
        if files.is_empty() {
            continue;
        }
        let (count, bytes) = if dry_run {
            (files.len(), files.iter().map(|(_, size)| size).sum())
        } else {
            let (count, bytes) = purge(&target.screens, &files);
            let body = serde_json::json!({ "at": Utc::now().to_rfc3339(), "files": count, "bytes": bytes, "kept": kept.len() });
            let _ = std::fs::write(target.screens.join(REPORT_FILE), body.to_string());
            info!("🧹 {} unused screenshot(s) deleted for {} ({} KB)", count, target.meeting_id, bytes / 1024);
            (count, bytes)
        };
        report.files += count;
        report.bytes += bytes;
        report.meetings.push(MeetingPurge {
            meeting_id: target.meeting_id,
            title: target.title,
            created_at: target.created_at,
            files: count,
            bytes,
            kept: kept.len(),
        });
    }
    Ok(report)
}

/// Background cleanup: shortly after start, then once a day.
pub fn start_worker<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_RUN_DELAY).await;
        loop {
            let pool = app.state::<AppState>().db_manager.pool().clone();
            let days = retention_days(&pool).await;
            if days > 0 {
                if let Err(e) = run(&pool, days, false).await {
                    warn!("Screenshot retention failed: {}", e);
                }
            }
            tokio::time::sleep(RUN_EVERY).await;
        }
    });
}

#[tauri::command]
pub async fn api_retention_preview(state: tauri::State<'_, AppState>) -> Result<RetentionReport, String> {
    let pool = state.db_manager.pool();
    run(pool, retention_days(pool).await, true).await
}

#[tauri::command]
pub async fn api_retention_run_now(state: tauri::State<'_, AppState>) -> Result<RetentionReport, String> {
    let pool = state.db_manager.pool();
    run(pool, retention_days(pool).await, false).await
}

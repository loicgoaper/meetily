// fork/meeting_time.rs
//
// When a meeting took place: its recording sessions (start, end, place on the
// transcript timeline), read from metadata.json. Feeds the fixed date line at the top
// of a summary, the date and times given to the summary model, and the meeting date.

use super::preferences::{PreferencesRepository, UI_LANGUAGE};
use crate::state::AppState;
use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, Timelike, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};

/// Preference set once the dates of older meetings were moved to their start
pub const CREATED_AT_FIXED: &str = "fork.created_at_from_metadata";
/// A start read in metadata.json replaces the saved date only within this window
const MAX_CORRECTION_HOURS: i64 = 12;

const FR_DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const FR_MONTHS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre",
];
const EN_DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const EN_MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December",
];

/// One recording session of a meeting (a resumed meeting has several).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// RFC 3339
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// Where the session starts on the transcript timeline (seconds)
    #[serde(default)]
    pub offset_seconds: f64,
    /// Active recording time (pauses excluded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct LocalSession {
    pub start: DateTime<FixedOffset>,
    pub end: Option<DateTime<FixedOffset>>,
    pub offset_seconds: f64,
    pub duration_seconds: Option<f64>,
}

impl LocalSession {
    /// Active recording time, else the wall-clock span
    pub fn duration(&self) -> Option<f64> {
        self.duration_seconds
            .or_else(|| self.end.map(|end| (end - self.start).num_seconds().max(0) as f64))
    }
}

fn parse(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.trim()).ok()
}

pub fn read_metadata(folder: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(folder.join("metadata.json")).ok()?;
    serde_json::from_str::<Value>(&raw).ok().filter(Value::is_object)
}

/// The `sessions` list, else one session rebuilt from created_at / completed_at.
pub fn sessions_from_metadata(meta: &Value) -> Vec<Session> {
    let listed: Vec<Session> = meta
        .get("sessions")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    if !listed.is_empty() {
        return listed;
    }
    let Some(start) = meta.get("created_at").and_then(Value::as_str).filter(|s| parse(s).is_some()) else {
        return Vec::new();
    };
    vec![Session {
        started_at: start.to_string(),
        ended_at: meta
            .get("completed_at")
            .and_then(Value::as_str)
            .filter(|s| parse(s).is_some())
            .map(str::to_string),
        offset_seconds: 0.0,
        duration_seconds: meta.get("duration_seconds").and_then(Value::as_f64),
    }]
}

pub fn localize(sessions: &[Session], to_local: impl Fn(DateTime<FixedOffset>) -> DateTime<FixedOffset>) -> Vec<LocalSession> {
    sessions
        .iter()
        .filter_map(|s| {
            Some(LocalSession {
                start: to_local(parse(&s.started_at)?),
                end: s.ended_at.as_deref().and_then(parse).map(&to_local),
                offset_seconds: s.offset_seconds,
                duration_seconds: s.duration_seconds,
            })
        })
        .collect()
}

/// Sessions in the Windows time zone.
pub fn local(sessions: &[Session]) -> Vec<LocalSession> {
    localize(sessions, |d| d.with_timezone(&Local).fixed_offset())
}

/// Sessions of a meeting: metadata.json of its folder, else its saved date.
pub async fn load(pool: &SqlitePool, meeting_id: &str) -> Vec<Session> {
    if let Ok(folder) = super::diarization::meeting_folder(pool, meeting_id).await {
        if let Some(meta) = read_metadata(&folder) {
            let sessions = sessions_from_metadata(&meta);
            if !sessions.is_empty() {
                return sessions;
            }
        }
    }
    let saved: Option<(String,)> = sqlx::query_as("SELECT created_at FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    saved
        .filter(|(created,)| parse(created).is_some())
        .map(|(created,)| vec![Session { started_at: created, ended_at: None, offset_seconds: 0.0, duration_seconds: None }])
        .unwrap_or_default()
}

fn day_label(d: &DateTime<FixedOffset>, french: bool, with_year: bool) -> String {
    let weekday = d.weekday().num_days_from_monday() as usize;
    let month = d.month0() as usize;
    let mut out = if french {
        let day = if d.day() == 1 { "1er".to_string() } else { d.day().to_string() };
        format!("{} {} {}", FR_DAYS[weekday], day, FR_MONTHS[month])
    } else {
        format!("{} {} {}", EN_DAYS[weekday], d.day(), EN_MONTHS[month])
    };
    if with_year {
        out.push_str(&format!(" {}", d.year()));
    }
    out
}

fn clock(d: &DateTime<FixedOffset>, french: bool) -> String {
    if french {
        format!("{} h {:02}", d.hour(), d.minute())
    } else {
        format!("{:02}:{:02}", d.hour(), d.minute())
    }
}

pub fn duration_label(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round().max(1.0) as i64;
    if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {:02}", minutes / 60, minutes % 60)
    }
}

fn total_duration(sessions: &[LocalSession]) -> Option<f64> {
    let known: Vec<f64> = sessions.iter().filter_map(LocalSession::duration).collect();
    (!known.is_empty()).then(|| known.iter().sum())
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// "*Vendredi 30 octobre 2026 · 14 h 05 – 14 h 40, reprise 16 h 02 – 16 h 30 · 1 h 03*"
pub fn header_line(sessions: &[LocalSession], french: bool) -> Option<String> {
    let first = sessions.first()?;
    let mut spans = Vec::with_capacity(sessions.len());
    for (index, s) in sessions.iter().enumerate() {
        let range = match s.end {
            Some(end) => format!("{} – {}", clock(&s.start, french), clock(&end, french)),
            None => clock(&s.start, french),
        };
        if index == 0 {
            spans.push(range);
            continue;
        }
        let word = if french { "reprise" } else { "resumed" };
        if s.start.date_naive() == first.start.date_naive() {
            spans.push(format!("{word} {range}"));
        } else {
            let on = if french { "le" } else { "on" };
            spans.push(format!("{word} {on} {} {range}", day_label(&s.start, french, false)));
        }
    }
    let mut parts = vec![capitalize(&day_label(&first.start, french, true)), spans.join(", ")];
    if let Some(total) = total_duration(sessions) {
        parts.push(duration_label(total));
    }
    Some(format!("*{}*", parts.join(" · ")))
}

/// True when a summary already starts with the date line.
pub fn starts_with_header(markdown: &str) -> bool {
    let first = markdown.trim_start().lines().next().unwrap_or("").trim();
    first.len() > 2 && first.starts_with('*') && !first.starts_with("**") && first.ends_with('*') && first.contains(" · ")
}

fn elapsed(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Date and sessions for the summary model (top of `<meeting_context>`).
pub fn prompt_lines(sessions: &[LocalSession], french: bool) -> Option<String> {
    let first = sessions.first()?;
    let mut out = format!(
        "Meeting date: {} ({}), local time UTC{}\n",
        day_label(&first.start, french, true),
        first.start.format("%Y-%m-%d"),
        first.start.format("%:z"),
    );
    for (index, s) in sessions.iter().enumerate() {
        let name = if index == 0 { format!("Session {}", index + 1) } else { format!("Session {} (resumed)", index + 1) };
        let day = if s.start.date_naive() == first.start.date_naive() {
            String::new()
        } else {
            format!("{} ", s.start.format("%Y-%m-%d"))
        };
        let clock_range = match s.end {
            Some(end) => format!("{}–{}", s.start.format("%H:%M"), end.format("%H:%M")),
            None => format!("from {}", s.start.format("%H:%M")),
        };
        let timeline = match s.duration() {
            Some(d) => format!("transcript [{}]–[{}]", elapsed(s.offset_seconds), elapsed(s.offset_seconds + d)),
            None => format!("transcript from [{}]", elapsed(s.offset_seconds)),
        };
        out.push_str(&format!("{name}: {day}{clock_range}, {timeline}\n"));
    }
    out.push_str("Transcript times [mm:ss] are elapsed recording time, not clock time.");
    Some(out)
}

/// Summary language when known, else the interface language (French by default).
pub async fn is_french(pool: &SqlitePool, language: Option<&str>) -> bool {
    match language.map(str::trim).filter(|l| !l.is_empty()) {
        Some(code) => code.to_ascii_lowercase().starts_with("fr"),
        None => PreferencesRepository::get(pool, UI_LANGUAGE).await != "en",
    }
}

/// Put the date line at the top of a generated summary (result_json["markdown"]).
pub async fn prepend_header(pool: &SqlitePool, meeting_id: &str, result: &mut Value, language: Option<&str>) {
    let Some(markdown) = result.get("markdown").and_then(Value::as_str).map(str::to_string) else {
        return;
    };
    if starts_with_header(&markdown) {
        return;
    }
    let sessions = local(&load(pool, meeting_id).await);
    let Some(header) = header_line(&sessions, is_french(pool, language).await) else {
        return;
    };
    result["markdown"] = Value::String(format!("{header}\n\n{}", markdown.trim_start()));
}

/// Start of the recording when it is a sane correction of the saved date (which is
/// the save time, at the end of the meeting).
pub fn start_correction(saved: &str, meta: &Value) -> Option<DateTime<Utc>> {
    let start = parse(meta.get("created_at")?.as_str()?)?.with_timezone(&Utc);
    let saved = parse(saved)?.with_timezone(&Utc);
    let earlier = saved - start;
    (earlier > Duration::zero() && earlier < Duration::hours(MAX_CORRECTION_HOURS)).then_some(start)
}

/// Date a freshly saved meeting by the start of its recording.
pub async fn align_created_at(pool: &SqlitePool, meeting_id: &str, folder: &Path) -> Result<bool, sqlx::Error> {
    let Some(meta) = read_metadata(folder) else {
        return Ok(false);
    };
    let saved: Option<(String,)> = sqlx::query_as("SELECT created_at FROM meetings WHERE id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;
    let Some(start) = saved.and_then(|(s,)| start_correction(&s, &meta)) else {
        return Ok(false);
    };
    sqlx::query("UPDATE meetings SET created_at = ? WHERE id = ?")
        .bind(start)
        .bind(meeting_id)
        .execute(pool)
        .await?;
    Ok(true)
}

/// One-time pass over meetings saved before perso.8 (dated by their end). The
/// database is copied to `backup_dir` first; nothing changes if the copy fails.
pub async fn fix_old_meetings(pool: &SqlitePool, backup_dir: &Path) -> Result<usize, String> {
    if PreferencesRepository::get(pool, CREATED_AT_FIXED).await == "done" {
        return Ok(0);
    }
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as("SELECT id, created_at, folder_path FROM meetings")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let fixes: Vec<(String, DateTime<Utc>)> = rows
        .into_iter()
        .filter_map(|(id, saved, folder)| {
            let meta = read_metadata(Path::new(&folder?))?;
            start_correction(&saved, &meta).map(|start| (id, start))
        })
        .collect();
    if !fixes.is_empty() {
        std::fs::create_dir_all(backup_dir).map_err(|e| e.to_string())?;
        let file = backup_dir.join(format!("meetily-avant-dates-de-debut-{}.sqlite", Utc::now().format("%Y%m%d-%H%M%S")));
        sqlx::query("VACUUM INTO ?")
            .bind(file.to_string_lossy().to_string())
            .execute(pool)
            .await
            .map_err(|e| format!("Sauvegarde de la base impossible : {e}"))?;
        let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
        for (id, start) in &fixes {
            sqlx::query("UPDATE meetings SET created_at = ? WHERE id = ?")
                .bind(start)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        for (id, _) in &fixes {
            super::notion::mark_dirty(id);
        }
    }
    let done = HashMap::from([(CREATED_AT_FIXED.to_string(), "done".to_string())]);
    PreferencesRepository::set_many(pool, &done).await.map_err(|e| e.to_string())?;
    Ok(fixes.len())
}

fn backup_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".meetily-perso").join("backups")
}

/// Run `fix_old_meetings` once, shortly after start-up.
pub fn start_old_meetings_fix<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        let pool = app.state::<AppState>().db_manager.pool().clone();
        match fix_old_meetings(&pool, &backup_dir()).await {
            Ok(0) => {}
            Ok(count) => info!("🗓️ {} meeting(s) now dated by the start of their recording", count),
            Err(e) => warn!("Meeting dates not corrected: {}", e),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn paris(d: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        d.with_timezone(&FixedOffset::east_opt(3600).unwrap())
    }

    fn session(start: &str, end: Option<&str>, offset: f64, duration: Option<f64>) -> Session {
        Session { started_at: start.into(), ended_at: end.map(String::from), offset_seconds: offset, duration_seconds: duration }
    }

    fn resumed_same_day() -> Vec<LocalSession> {
        localize(
            &[
                session("2026-10-30T13:05:00+00:00", Some("2026-10-30T13:40:00+00:00"), 0.0, Some(2112.0)),
                session("2026-10-30T15:02:00+00:00", Some("2026-10-30T15:30:00+00:00"), 2112.0, Some(1680.0)),
            ],
            paris,
        )
    }

    #[test]
    fn sessions_are_read_or_rebuilt_from_metadata() {
        let listed = json!({ "created_at": "x", "sessions": [ { "started_at": "2026-10-30T13:05:00+00:00", "offset_seconds": 0.0 } ] });
        assert_eq!(sessions_from_metadata(&listed).len(), 1);

        let single = json!({ "created_at": "2026-10-30T13:05:00+00:00", "completed_at": "2026-10-30T14:12:00+00:00", "duration_seconds": 4020.0 });
        assert_eq!(
            sessions_from_metadata(&single),
            vec![session("2026-10-30T13:05:00+00:00", Some("2026-10-30T14:12:00+00:00"), 0.0, Some(4020.0))]
        );

        assert!(sessions_from_metadata(&json!({ "meeting_name": "x" })).is_empty());
    }

    #[test]
    fn header_for_one_session_in_french() {
        let sessions = localize(&[session("2026-10-30T13:05:00+00:00", Some("2026-10-30T14:12:00+00:00"), 0.0, Some(4020.0))], paris);
        assert_eq!(header_line(&sessions, true).unwrap(), "*Vendredi 30 octobre 2026 · 14 h 05 – 15 h 12 · 1 h 07*");
    }

    #[test]
    fn header_for_a_resumed_meeting() {
        assert_eq!(
            header_line(&resumed_same_day(), true).unwrap(),
            "*Vendredi 30 octobre 2026 · 14 h 05 – 14 h 40, reprise 16 h 02 – 16 h 30 · 1 h 03*"
        );
        let next_day = localize(
            &[
                session("2026-10-30T13:05:00+00:00", Some("2026-10-30T13:40:00+00:00"), 0.0, Some(2100.0)),
                session("2026-10-31T08:00:00+00:00", Some("2026-10-31T08:30:00+00:00"), 2100.0, Some(1800.0)),
            ],
            paris,
        );
        assert_eq!(
            header_line(&next_day, false).unwrap(),
            "*Friday 30 October 2026 · 14:05 – 14:40, resumed on Saturday 31 October 09:00 – 09:30 · 1 h 05*"
        );
        assert_eq!(
            header_line(&next_day, true).unwrap(),
            "*Vendredi 30 octobre 2026 · 14 h 05 – 14 h 40, reprise le samedi 31 octobre 9 h 00 – 9 h 30 · 1 h 05*"
        );
    }

    #[test]
    fn header_without_end_and_first_of_month() {
        let sessions = localize(&[session("2026-11-01T09:00:00+00:00", None, 0.0, None)], paris);
        assert_eq!(header_line(&sessions, true).unwrap(), "*Dimanche 1er novembre 2026 · 10 h 00*");
        assert!(header_line(&[], true).is_none());
    }

    #[test]
    fn header_is_recognised() {
        let header = header_line(&resumed_same_day(), true).unwrap();
        assert!(starts_with_header(&format!("{header}\n\n## Résumé")));
        assert!(!starts_with_header("**Résumé** · point"));
        assert!(!starts_with_header("## Résumé"));
    }

    #[test]
    fn prompt_gives_date_sessions_and_timeline() {
        let lines = prompt_lines(&resumed_same_day(), true).unwrap();
        assert!(lines.starts_with("Meeting date: vendredi 30 octobre 2026 (2026-10-30), local time UTC+01:00\n"));
        assert!(lines.contains("Session 1: 14:05–14:40, transcript [00:00]–[35:12]\n"));
        assert!(lines.contains("Session 2 (resumed): 16:02–16:30, transcript [35:12]–[63:12]\n"));
        assert!(lines.ends_with("not clock time."));
    }

    #[test]
    fn durations_are_rounded_to_minutes() {
        assert_eq!(duration_label(20.0), "1 min");
        assert_eq!(duration_label(3564.0), "59 min");
        assert_eq!(duration_label(3600.0), "1 h 00");
    }

    #[test]
    fn start_correction_stays_within_twelve_hours() {
        let meta = json!({ "created_at": "2026-10-30T13:05:00+00:00" });
        assert!(start_correction("2026-10-30T14:12:00+00:00", &meta).is_some());
        assert!(start_correction("2026-10-30T13:00:00+00:00", &meta).is_none(), "saved before the start");
        assert!(start_correction("2026-10-31T03:00:00+00:00", &meta).is_none(), "more than 12 h");
        assert!(start_correction("2026-10-30", &meta).is_none(), "unreadable saved date");
    }
}

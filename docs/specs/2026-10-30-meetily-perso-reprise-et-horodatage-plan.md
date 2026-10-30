# Reprise de réunion et horodatage — plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** reprendre une réunion arrêtée dans la même réunion (audio, transcription, captures, chaîne de fin), et donner la date et l'heure réelles aux comptes rendus (en-tête fixe et modèle informé).

**Architecture :**
- Deux modules Rust du fork.
  - `fork/meeting_time.rs` lit les sessions d'une réunion dans `metadata.json` et produit l'en-tête, le bloc du prompt et la correction de date.
  - `fork/resume.rs` tient l'état d'une reprise. Il redirige la session vers `.reprise-N/`, décale la chronologie, puis recolle la session à l'arrêt.
- Le code officiel ne reçoit que des points d'accroche d'une ou deux lignes : saver, worker, capture d'écran, arrêt, sauvegarde, résumé.
- Côté interface :
  - un bouton dans `MeetingCloudBar` ;
  - un mode « reprise » de la fenêtre de démarrage ;
  - un bandeau pendant l'enregistrement ;
  - `source=resume` à l'arrêt.

**Tech Stack :** Rust (Tauri 2, sqlx SQLite, chrono, serde_json, ffmpeg), Next.js 14 / React 18 / TypeScript, i18next, bun test.

## Global Constraints

- Spec : `docs/specs/2026-10-30-meetily-perso-reprise-et-horodatage-design.md`.
- Version livrée : `0.4.1-perso.8` (`frontend/src-tauri/tauri.conf.json`).
- Ne jamais modifier les fichiers de la réunion d'origine avant un recollage réussi. `audio.mp4` n'est remplacé que par renommage d'un fichier vérifié (durée = somme ± 2 s).
- Pas de nouvelle dépendance Cargo ni npm.
- Chaînes d'interface en FR et EN (`frontend/src/i18n/locales/{fr,en}/fork.json`). Messages d'erreur Rust en français, comme le reste du fork.
- Compilation Rust sous Windows : `frontend\scripts\fork-dev-env.cmd <commande>` (environnement MSVC + libclang).
- Tests Rust du fork : `frontend\scripts\fork-dev-env.cmd cargo test --manifest-path frontend\src-tauri\Cargo.toml --lib fork::`. Le test officiel `device_detection` bluetooth échoue déjà et est hors sujet.
- Tests interface : `cd frontend && bun test`, `npx tsc --noEmit`, `node scripts/check-i18n-keys.mjs`.
- Ne jamais arrêter ni réinstaller Meetily pendant un enregistrement : avant toute installation, vérifier l'absence de dossier `.checkpoints` récent et de compte rendu en cours.
- Commits avec la ligne `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

### Task 1 : `fork/meeting_time.rs`, sessions, en-tête, prompt, correction de date

**Files :**
- Create : `frontend/src-tauri/src/fork/meeting_time.rs`
- Modify : `frontend/src-tauri/src/fork/mod.rs` (ajouter `pub mod meeting_time;`)
- Test : tests dans le module, plus `frontend/src-tauri/src/fork/db_tests.rs`

**Interfaces :**
- Produces :
  - `Session { started_at: String, ended_at: Option<String>, offset_seconds: f64, duration_seconds: Option<f64> }` (Serialize/Deserialize/Clone/PartialEq)
  - `LocalSession` et `LocalSession::duration()`
  - `read_metadata(&Path) -> Option<Value>`, `sessions_from_metadata(&Value) -> Vec<Session>`
  - `localize(..)`, `local(&[Session]) -> Vec<LocalSession>`, `load(&SqlitePool, &str) -> Vec<Session>`
  - `header_line(&[LocalSession], bool) -> Option<String>`, `starts_with_header(&str) -> bool`, `prompt_lines(&[LocalSession], bool) -> Option<String>`
  - `is_french(&SqlitePool, Option<&str>) -> bool`, `prepend_header(&SqlitePool, &str, &mut Value, Option<&str>)`
  - `start_correction(&str, &Value) -> Option<DateTime<Utc>>`, `align_created_at(&SqlitePool, &str, &Path) -> Result<bool, sqlx::Error>`
  - `fix_old_meetings(&SqlitePool, &Path) -> Result<usize, String>`, `start_old_meetings_fix(AppHandle<R>)`, `CREATED_AT_FIXED`

- [ ] **Step 1 : écrire le module avec ses tests (TDD : tests d'abord, corps des fonctions `todo!()`, voir l'échec, puis le code ci-dessous)**

```rust
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
```

- [ ] **Step 2 : tests base de données dans `fork/db_tests.rs`** (à la fin du fichier)

```rust
fn write_metadata(dir: &std::path::Path, value: serde_json::Value) {
    std::fs::write(dir.join("metadata.json"), value.to_string()).unwrap();
}

#[tokio::test]
async fn meeting_is_dated_by_the_start_of_its_recording() {
    let pool = migrated_pool().await;
    let folder = tempfile::tempdir().unwrap();
    write_metadata(folder.path(), serde_json::json!({ "created_at": "2026-10-30T13:05:00+00:00" }));
    sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES ('m2', 'R', '2026-10-30T14:12:00+00:00', '2026-10-30T14:12:00+00:00', ?)")
        .bind(folder.path().to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();

    assert!(super::meeting_time::align_created_at(&pool, "m2", folder.path()).await.unwrap());
    let (created,): (String,) = sqlx::query_as("SELECT created_at FROM meetings WHERE id = 'm2'").fetch_one(&pool).await.unwrap();
    assert!(created.starts_with("2026-10-30T13:05:00"), "{created}");

    let sessions = super::meeting_time::load(&pool, "m2").await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].started_at, "2026-10-30T13:05:00+00:00");
}

#[tokio::test]
async fn older_meetings_are_fixed_once_after_a_backup() {
    let pool = migrated_pool().await;
    let folder = tempfile::tempdir().unwrap();
    write_metadata(folder.path(), serde_json::json!({ "created_at": "2026-10-30T13:05:00+00:00" }));
    sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES ('m3', 'R', '2026-10-30T14:12:00+00:00', '2026-10-30T14:12:00+00:00', ?)")
        .bind(folder.path().to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();
    let backups = tempfile::tempdir().unwrap();

    assert_eq!(super::meeting_time::fix_old_meetings(&pool, backups.path()).await.unwrap(), 1);
    assert_eq!(std::fs::read_dir(backups.path()).unwrap().count(), 1, "database copied first");
    let (created,): (String,) = sqlx::query_as("SELECT created_at FROM meetings WHERE id = 'm3'").fetch_one(&pool).await.unwrap();
    assert!(created.starts_with("2026-10-30T13:05:00"));
    // m1 (no folder) is untouched, and the pass never runs twice
    assert_eq!(super::meeting_time::fix_old_meetings(&pool, backups.path()).await.unwrap(), 0);
}
```

- [ ] **Step 3 : lancer les tests**

Run : `frontend\scripts\fork-dev-env.cmd cargo test --manifest-path frontend\src-tauri\Cargo.toml --lib fork::meeting_time fork::db_tests`

Expected : tous verts.

Note : `VACUUM INTO` sur une base `sqlite::memory:` produit un fichier, ce qui suffit au test.

- [ ] **Step 4 : commit** `feat(fork): meeting sessions, date line and start-of-meeting dates`

---

### Task 2 : date et heure dans le compte rendu

**Files :**
- Modify :
  - `frontend/src-tauri/src/fork/summary_context.rs` (signature et bloc horaire)
  - `frontend/src-tauri/src/summary/commands.rs:495` (passer la langue)
  - `frontend/src-tauri/src/summary/processor.rs:275` (règle 4)
  - `frontend/src-tauri/src/summary/service.rs:~605` (en-tête après les captures)
  - `frontend/src-tauri/src/fork/meeting_doc.rs:191` (pas de date en double à l'export)
  - `frontend/src-tauri/src/api/api.rs` (`api_save_transcript` : date de début)
  - `frontend/src-tauri/src/lib.rs:601` (correction unique au démarrage)
- Test : `processor.rs` (tests du module) et `fork/db_tests.rs`

**Interfaces :**
- Consumes : Task 1 (`meeting_time::*`).
- Produces : `summary_context::build(pool, meeting_id, user_instructions, screen_budget, summary_language: Option<&str>) -> String`.

- [ ] **Step 1 : test du prompt (processor.rs, module `tests`)**

```rust
    #[test]
    fn fork_final_prompt_uses_meeting_dates() {
        let prompt = build_final_report_system_prompt("", "", "Write in French.", "Aucun");
        assert!(prompt.contains("relative dates"));
        assert!(prompt.contains("[mm:ss]"));
    }
```

- [ ] **Step 2 : test de `summary_context::build` (db_tests.rs)**

```rust
#[tokio::test]
async fn summary_context_always_gives_the_meeting_date() {
    let pool = migrated_pool().await;
    let folder = tempfile::tempdir().unwrap();
    write_metadata(folder.path(), serde_json::json!({ "created_at": "2026-10-30T13:05:00+00:00", "completed_at": "2026-10-30T14:12:00+00:00", "duration_seconds": 4020.0 }));
    sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES ('m4', 'R', '2026-10-30T13:05:00+00:00', '2026-10-30T13:05:00+00:00', ?)")
        .bind(folder.path().to_string_lossy().to_string())
        .execute(&pool)
        .await
        .unwrap();
    let block = super::summary_context::build(&pool, "m4", None, 1000, Some("fr")).await;
    assert!(block.starts_with("<meeting_context>\nMeeting date: vendredi 30 octobre 2026"), "{block}");
    assert!(block.contains("transcript [00:00]–[67:00]"));
}
```

- [ ] **Step 3 : vérifier l'échec**, puis implémenter.

`fork/summary_context.rs` : signature et rendu du bloc.

```rust
pub async fn build(
    pool: &SqlitePool,
    meeting_id: &str,
    user_instructions: Option<&str>,
    screen_budget: usize,
    summary_language: Option<&str>,
) -> String {
    // ... chargement du contexte inchangé ...
    let mut blocks: Vec<String> = Vec::new();
    // Fork: date, time and sessions first, so dates and deadlines can be written out
    let sessions = super::meeting_time::local(&super::meeting_time::load(pool, meeting_id).await);
    let french = super::meeting_time::is_french(pool, summary_language).await;
    let mut rendered = super::meeting_time::prompt_lines(&sessions, french).unwrap_or_default();
    let context_lines = context.render_for_prompt();
    if !context_lines.is_empty() {
        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&context_lines);
    }
    if !rendered.is_empty() {
        blocks.push(format!("<meeting_context>\n{}\n</meeting_context>", rendered));
    }
    // ... screen_content et user_instructions inchangés ...
}
```

`summary/commands.rs:495` :

```rust
    let final_prompt = crate::fork::summary_context::build(&pool, &m_id, custom_prompt.as_deref(), screen_budget, summary_language.as_deref()).await;
```

`summary/processor.rs`, règle 4 du prompt final : ajouter à la fin de la ligne 4 actuelle :

```text
 Dates and times of the meeting are given at the top of `<meeting_context>`: use them for any date or time field of the template, and write relative dates said in the meeting ("tomorrow", "next Friday") as absolute dates. Keep [mm:ss] for references to moments of the meeting.
```

`summary/service.rs`, juste après l'appel `crate::fork::illustrate::illustrate_result_json(...).await;` :

```rust
                // Fork: fixed date line, written by Meetily (never by the model)
                crate::fork::meeting_time::prepend_header(
                    &pool,
                    &meeting_id,
                    &mut result_json,
                    summary_language.as_deref().or(detected_summary_language.as_deref()),
                )
                .await;
```

`fork/meeting_doc.rs:191`, export du compte rendu :

```rust
        // The summary may already start with the date line (perso.8)
        if super::meeting_time::starts_with_header(&summary) {
            Some(format!("# {}\n\n{}\n", self.title, summary))
        } else {
            Some(format!("# {}\n\n*{}*\n\n{}\n", self.title, self.date_label(), summary))
        }
```

Adapter au nom réel de la variable du compte rendu à cette ligne.

`api/api.rs`, `api_save_transcript` :
- avant l'appel à `TranscriptsRepository::save_transcript`, ajouter `let folder_for_date = folder_path.clone();` ;
- dans `Ok(meeting_id) =>`, avant `persist_new_meeting_extras`, ajouter :

```rust
            // Fork: a meeting is dated by the start of its recording, not by this save
            if let Some(folder) = folder_for_date.as_deref() {
                if let Err(e) = crate::fork::meeting_time::align_created_at(pool, &meeting_id, std::path::Path::new(folder)).await {
                    log_warn!("Could not date meeting {} by its start: {}", meeting_id, e);
                }
            }
```

Utiliser la macro de log disponible dans `api.rs` : `log_warn!`, sinon `log_info!`.

`lib.rs`, après `fork::retention::start_worker(...)` :

```rust
            fork::meeting_time::start_old_meetings_fix(_app.handle().clone());
```

- [ ] **Step 4 : tests**

Run :
- `cargo test --lib summary::processor fork::` (via `fork-dev-env.cmd`) ;
- `cargo check`.

Expected : verts. Les tests existants du prompt (`fork_native_french_prompt_has_french_rules_and_context_tags`) passent toujours.

- [ ] **Step 5 : commit** `feat(fork): summaries know the meeting date and start with a date line`

---

### Task 3 : `fork/resume.rs`, état, recollage audio, fusion des fichiers

**Files :**
- Create : `frontend/src-tauri/src/fork/resume.rs`
- Modify :
  - `frontend/src-tauri/src/fork/mod.rs` (`pub mod resume;`)
  - `frontend/src-tauri/src/fork/source_timeline.rs` (`pub fn write(folder, &SourceTimeline)`)
  - `frontend/src-tauri/src/fork/diarization/mod.rs` (`ffmpeg_command` en `pub(crate)`, `pub fn is_running()`)
- Test : tests dans le module (ffmpeg réel, ignoré s'il est absent), `fork/db_tests.rs`

**Interfaces :**
- Consumes : `meeting_time::{Session, read_metadata, sessions_from_metadata}`, `source_timeline::{SourceTimeline, load_from_folder, write}`, `diarization::{audio_duration, meeting_folder, ffmpeg_command, is_running}`.
- Produces :
  - type `ResumeTarget { meeting_id, title, folder: PathBuf, session_dir: PathBuf, offset_seconds: f64, session_number: u32 }`, sérialisé en camelCase ;
  - état : `active() -> Option<ResumeTarget>`, `session_folder() -> Option<PathBuf>`, `offset() -> f64`, `capture_folder() -> Option<PathBuf>` ;
  - arrêt et sauvegarde : `finish_after_stop(&AppHandle<R>) -> Option<PathBuf>`, `take_finished_for(Option<&str>) -> Option<ResumeTarget>`, `append_to_meeting(&SqlitePool, &ResumeTarget, &[TranscriptSegment], Option<&MeetingContext>) -> Result<(), String>` ;
  - garde : `unmerged_session(&Path) -> Option<PathBuf>` ;
  - commandes Tauri `api_check_resume(meeting_id)`, `api_prepare_resume(meeting_id) -> ResumeTarget`, `api_cancel_resume()`, `api_active_resume() -> Option<ResumeTarget>` ;
  - fonctions pures : `next_session_number`, `merge_timelines`, `add_session`, `concat_audio`, `append_live_backup`.

- [ ] **Step 1 : petites ouvertures dans les modules existants**

`fork/source_timeline.rs`, remplacer le corps de `save_to_folder` :

```rust
pub fn save_to_folder(folder: &Path) -> std::io::Result<()> {
    write(folder, &snapshot())
}

/// Write a timeline next to a recording (empty timelines are not written).
pub fn write(folder: &Path, timeline: &SourceTimeline) -> std::io::Result<()> {
    if timeline.windows.is_empty() {
        return Ok(());
    }
    let json = serde_json::to_string(timeline)?;
    std::fs::write(folder.join("source_energy.json"), json)
}
```

`fork/diarization/mod.rs` :
- `fn ffmpeg_command()` devient `pub(crate) fn ffmpeg_command()` ;
- ajouter :

```rust
/// True while a speaker identification runs (a resume must wait for it).
pub fn is_running() -> bool {
    IN_PROGRESS.load(Ordering::SeqCst)
}
```

Dans `diarize_meeting`, juste après `let folder = meeting_folder(&pool, meeting_id).await?;` :

```rust
    // Fork: never re-transcribe audio.mp4 while a resumed session is not glued onto it
    // (its part of the meeting would vanish from the transcript)
    if let Some(dir) = super::resume::unmerged_session(&folder) {
        return Err(format!(
            "L'audio de la reprise ({}) n'est pas recollé à la réunion : identification annulée pour ne pas la perdre",
            dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        ));
    }
```

- [ ] **Step 2 : le module, tests d'abord**

```rust
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
```

Note : `TranscriptSegment` de `crate::api` a des champs publics, sans `Clone` ; les tests le construisent directement.

- [ ] **Step 3 : test d'ajout en base (`fork/db_tests.rs`)**

```rust
#[tokio::test]
async fn resumed_session_is_appended_to_its_meeting() {
    let pool = migrated_pool().await;
    let folder = tempfile::tempdir().unwrap();
    let target = super::resume::ResumeTarget {
        meeting_id: "m1".into(),
        title: "Réunion".into(),
        folder: folder.path().to_path_buf(),
        session_dir: folder.path().join(".reprise-2"),
        offset_seconds: 60.0,
        session_number: 2,
    };
    let segment = crate::api::TranscriptSegment {
        id: "s".into(),
        text: "suite".into(),
        timestamp: "16:02:00".into(),
        audio_start_time: Some(61.0),
        audio_end_time: Some(63.0),
        duration: Some(2.0),
        speaker: Some("mic".into()),
    };
    super::resume::append_to_meeting(&pool, &target, &[segment], None).await.unwrap();
    let (count, start): (i64, f64) = sqlx::query_as("SELECT COUNT(*), MAX(audio_start_time) FROM transcripts WHERE meeting_id = 'm1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((count, start), (1, 61.0));
}
```

- [ ] **Step 4 : lancer**

Run : `fork-dev-env.cmd cargo test --manifest-path frontend\src-tauri\Cargo.toml --lib fork::resume fork::db_tests`

Expected : verts. Les tests de recollage tournent avec le vrai ffmpeg du poste ; s'il est absent, ils se signalent comme sautés.

- [ ] **Step 5 : commit** `feat(fork): resume state, audio glue and file merges`

---

### Task 4 : points d'accroche Rust de la reprise

**Files :**
- `frontend/src-tauri/src/audio/recording_saver.rs` (`initialize_meeting_folder`)
- `frontend/src-tauri/src/audio/transcription/worker.rs:200-202`
- `frontend/src-tauri/src/fork/screen_capture.rs` (`on_recording_started`, `start`, `capture_loop`)
- `frontend/src-tauri/src/audio/recording_commands.rs` (`stop_recording`, après le bloc `manager_for_cleanup`)
- `frontend/src-tauri/src/api/api.rs` (`api_save_transcript`)
- `frontend/src-tauri/src/lib.rs` (enregistrement des 4 commandes)

**Interfaces :**
- Consumes : Task 3.
- Produces : réponse de `api_save_transcript` `{ status, message, meeting_id, resumed: true }` pour une reprise ; événements `resume-merge-failed { meetingId, message }`.

- [ ] **Step 1 : saver.** Dans `initialize_meeting_folder`, remplacer `let meeting_folder = create_meeting_folder(&base_folder, meeting_name, create_checkpoints)?;` par :

```rust
        // Fork: a resumed meeting records its new session inside the meeting's folder
        let meeting_folder = match crate::fork::resume::session_folder() {
            Some(folder) => {
                std::fs::create_dir_all(if create_checkpoints { folder.join(".checkpoints") } else { folder.clone() })?;
                folder
            }
            None => create_meeting_folder(&base_folder, meeting_name, create_checkpoints)?,
        };
```

- [ ] **Step 2 : worker.** Lignes 200-202 :

```rust
                                        let audio_start_time = chunk_timestamp; // Already in seconds from recording start
                                        let audio_end_time = chunk_timestamp + chunk_duration;
                                        let speaker = crate::fork::source_timeline::label(audio_start_time, audio_end_time);
                                        // Fork: a resumed meeting continues the timeline of its earlier sessions
                                        let resume_offset = crate::fork::resume::offset();
                                        let audio_start_time = audio_start_time + resume_offset;
                                        let audio_end_time = audio_end_time + resume_offset;
```

- [ ] **Step 3 : captures d'écran**

Dans `on_recording_started`, première ligne du corps :

```rust
    // Fork: screenshots of a resumed session go to the meeting's own screens/
    let folder = super::resume::capture_folder().or(folder);
```

Dans `start()` :
- remplacer `let frames = Arc::new(AtomicU32::new(existing.len() as u32));` par :

```rust
    // The 300-capture limit applies per session (a resumed meeting keeps its earlier ones)
    let frames = Arc::new(AtomicU32::new(0));
    let time_offset = super::resume::offset();
```

- ajouter `time_offset` en dernier argument de `capture_loop(...)`.

Dans `capture_loop` :
- ajouter le paramètre `time_offset: f64` ;
- remplacer `let t = state.get_active_recording_duration().unwrap_or(0.0);` par :

```rust
        let t = state.get_active_recording_duration().unwrap_or(0.0) + time_offset;
```

- [ ] **Step 4 : arrêt.** Dans `stop_recording`, juste après la fin du bloc `let (meeting_folder, meeting_name) = if let Some(mut manager) = manager_for_cleanup { ... } else { ... };` :

```rust
    // Fork: a resumed session is glued onto its meeting, whose folder is reported
    let meeting_folder = crate::fork::resume::finish_after_stop(&app).or(meeting_folder);
```

- [ ] **Step 5 : sauvegarde.** Dans `api_save_transcript`, juste après `let pool = state.db_manager.pool();` :

```rust
    // Fork: a resumed meeting appends its new session instead of creating a meeting
    if let Some(target) = crate::fork::resume::take_finished_for(folder_path.as_deref()) {
        crate::fork::resume::append_to_meeting(pool, &target, &transcripts_to_save, context.as_ref()).await?;
        return Ok(serde_json::json!({
            "status": "success",
            "message": "Transcript appended to the resumed meeting",
            "meeting_id": target.meeting_id,
            "resumed": true
        }));
    }
```

- [ ] **Step 6 : enregistrement des commandes** dans `generate_handler![...]` de `lib.rs`, à côté des autres commandes `fork::` :

```rust
            fork::resume::api_check_resume,
            fork::resume::api_prepare_resume,
            fork::resume::api_cancel_resume,
            fork::resume::api_active_resume,
```

- [ ] **Step 7 : `cargo check` et tests du fork** (via `fork-dev-env.cmd`).

Expected : compilation propre et tests verts. Si `capture_loop` dépasse la limite d'arguments de clippy, l'attribut `#[allow(clippy::too_many_arguments)]` est déjà présent.

- [ ] **Step 8 : commit** `feat(fork): recording, screenshots, stop and save follow a resumed meeting`

---

### Task 5 : interface, services et logique pure

**Files :**
- Modify : `frontend/src/services/forkService.ts`, `frontend/src/services/storageService.ts` (`resumed?: boolean`), `frontend/src/hooks/fork/usePostMeetingPipeline.ts`, `frontend/src/i18n/locales/fr/fork.json`, `frontend/src/i18n/locales/en/fork.json`
- Create : `frontend/src/lib/fork/resume.ts`
- Test : `frontend/tests/lib/resume.test.ts`

**Interfaces :**
- Produces :
  - `requestResume(req, storage?)`, `takeResumeRequest(storage?) -> ResumeRequest | null`
  - `sourceAfterSave({resumed?}) -> 'recording' | 'resume'`, `canAutoGenerate(status, resumed) -> boolean`, `formatElapsed(seconds) -> string`
  - `checkResume`, `prepareResume`, `cancelResume`, `getActiveResume`, type `ResumeTarget`
  - `forgetPostMeetingRun(meetingId)`

- [ ] **Step 1 : test bun `frontend/tests/lib/resume.test.ts`**

```ts
import { describe, expect, test } from 'bun:test';
import { canAutoGenerate, formatElapsed, requestResume, sourceAfterSave, takeResumeRequest } from '@/lib/fork/resume';

function memoryStorage() {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
  };
}

describe('resume a stopped meeting', () => {
  test('the request is read once and arms the auto-start', () => {
    const storage = memoryStorage();
    requestResume({ meetingId: 'meeting-1', title: 'Point client' }, storage);
    expect(storage.data.get('autoStartRecording')).toBe('true');
    expect(takeResumeRequest(storage)).toEqual({ meetingId: 'meeting-1', title: 'Point client' });
    expect(takeResumeRequest(storage)).toBeNull();
  });

  test('a damaged request is ignored', () => {
    const storage = memoryStorage();
    storage.setItem('fork.resumeMeeting', '{oops');
    expect(takeResumeRequest(storage)).toBeNull();
  });

  test('a resumed save opens the meeting as resumed', () => {
    expect(sourceAfterSave({ resumed: true })).toBe('resume');
    expect(sourceAfterSave({})).toBe('recording');
  });

  test('the automatic summary replaces the summary of a resumed meeting only', () => {
    expect(canAutoGenerate('idle', false)).toBe(true);
    expect(canAutoGenerate('completed', false)).toBe(false);
    expect(canAutoGenerate('completed', true)).toBe(true);
    expect(canAutoGenerate('error', true)).toBe(true);
    expect(canAutoGenerate('processing', true)).toBe(false);
  });

  test('elapsed time continues past an hour', () => {
    expect(formatElapsed(2112)).toBe('35:12');
    expect(formatElapsed(3792.7)).toBe('63:12');
  });
});
```

- [ ] **Step 2 : `bun test tests/lib/resume.test.ts`** échoue, le module étant absent.

- [ ] **Step 3 : `frontend/src/lib/fork/resume.ts`**

```ts
// Meetily perso: resume a stopped meeting from its page. The request crosses the
// navigation to the recording page through sessionStorage, like the sidebar start.

const RESUME_KEY = 'fork.resumeMeeting';

export interface ResumeRequest {
  meetingId: string;
  title: string;
}

type Writable = Pick<Storage, 'setItem'>;
type Readable = Pick<Storage, 'getItem' | 'removeItem'>;

/** Ask the recording page to resume this meeting (picked up by its auto-start). */
export function requestResume(request: ResumeRequest, storage: Writable = sessionStorage) {
  storage.setItem(RESUME_KEY, JSON.stringify(request));
  storage.setItem('autoStartRecording', 'true');
}

/** The pending resume request, consumed. */
export function takeResumeRequest(storage: Readable = sessionStorage): ResumeRequest | null {
  const raw = storage.getItem(RESUME_KEY);
  storage.removeItem(RESUME_KEY);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    return typeof parsed?.meetingId === 'string' && typeof parsed?.title === 'string'
      ? { meetingId: parsed.meetingId, title: parsed.title }
      : null;
  } catch {
    return null;
  }
}

/** Meeting page source after a stop: a resumed meeting regenerates its summary. */
export function sourceAfterSave(response: { resumed?: boolean }): 'recording' | 'resume' {
  return response.resumed ? 'resume' : 'recording';
}

/** The automatic summary runs on a new meeting, or replaces the one of a resumed meeting. */
export function canAutoGenerate(status: string, resumed: boolean): boolean {
  return status === 'idle' || (resumed && (status === 'completed' || status === 'error'));
}

/** "[mm:ss]" of the meeting timeline (minutes keep counting past an hour). */
export function formatElapsed(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
}
```

- [ ] **Step 4 : services**

`forkService.ts`, à la fin :

```ts
// --- Resume a stopped meeting -----------------------------------------------------

export interface ResumeTarget {
  meetingId: string;
  title: string;
  folder: string;
  sessionDir: string;
  offsetSeconds: number;
  sessionNumber: number;
}

/** Throws the reason when the meeting cannot be resumed now. */
export const checkResume = (meetingId: string) => invoke<void>('api_check_resume', { meetingId });
export const prepareResume = (meetingId: string) => invoke<ResumeTarget>('api_prepare_resume', { meetingId });
export const cancelResume = () => invoke<void>('api_cancel_resume');
export const getActiveResume = () => invoke<ResumeTarget | null>('api_active_resume');
```

`storageService.ts` :

```ts
export interface SaveMeetingResponse {
  meeting_id: string;
  /** Meetily perso: the recording resumed an existing meeting */
  resumed?: boolean;
}
```

`usePostMeetingPipeline.ts`, après `const activeRuns = ...` :

```ts
/** A resumed meeting must run the whole post-meeting pipeline again. */
export function forgetPostMeetingRun(meetingId: string) {
  processedMeetings.delete(meetingId);
}
```

- [ ] **Step 5 : traductions** (bloc `resume` dans `fork.json`)

FR :

```json
  "resume": {
    "button": "Reprendre la réunion",
    "recordingInProgress": "Un enregistrement est déjà en cours",
    "pipelineBusy": "Attendez la fin de l'identification des intervenants ou de la lecture des captures",
    "summaryBusy": "Attendez la fin de la génération du compte rendu",
    "dialogTitle": "Reprendre « {{title}} »",
    "dialogWarning": "La suite s'ajoute à cette réunion. À l'arrêt, les intervenants et le compte rendu seront refaits sur toute la réunion : les corrections manuelles seront remplacées.",
    "start": "Reprendre l'enregistrement",
    "banner": "Reprise de « {{title}} » · la suite commence à [{{time}}]",
    "mergeFailed": "L'audio de la reprise n'a pas pu être recollé à la réunion"
  },
```

EN :

```json
  "resume": {
    "button": "Resume meeting",
    "recordingInProgress": "A recording is already in progress",
    "pipelineBusy": "Wait until speaker identification or screenshot reading is over",
    "summaryBusy": "Wait until the summary is generated",
    "dialogTitle": "Resume \"{{title}}\"",
    "dialogWarning": "The recording is added to this meeting. When it stops, speakers and the summary are redone for the whole meeting: manual corrections are replaced.",
    "start": "Resume recording",
    "banner": "Resuming \"{{title}}\" · continues at [{{time}}]",
    "mergeFailed": "The resumed audio could not be added to the meeting"
  },
```

- [ ] **Step 6 : vérifier**

Run : `cd frontend && bun test tests/lib/resume.test.ts && node scripts/check-i18n-keys.mjs`

Expected : 5 tests verts, clés équilibrées FR/EN.

- [ ] **Step 7 : commit** `feat(fork): resume request, services and translations`

---

### Task 6 : interface, bouton, fenêtre de reprise, bandeau, arrêt et page

**Files :**
- Create : `frontend/src/components/fork/ResumeMeetingButton.tsx`, `frontend/src/components/fork/ResumeBanner.tsx`
- Modify :
  - `frontend/src/components/fork/MeetingCloudBar.tsx` et `frontend/src/app/meeting-details/page-content.tsx` (props et effet d'auto-génération)
  - `frontend/src/app/meeting-details/page.tsx` (`source=resume`)
  - `frontend/src/contexts/MeetingSetupContext.tsx` (mode reprise)
  - `frontend/src/hooks/useRecordingStart.ts` (chemin auto-start)
  - `frontend/src/hooks/useRecordingStop.ts` (réponse `resumed`)
  - `frontend/src/app/page.tsx` (bandeau)

**Interfaces :**
- Consumes : Task 5.
- Produces : `requestSetup(options?: { resume?: ResumeRequest })`.

- [ ] **Step 1 : `ResumeMeetingButton.tsx`**

```tsx
'use client';

import { useRouter } from 'next/navigation';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { PlayCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { checkResume } from '@/services/forkService';
import { requestResume } from '@/lib/fork/resume';

/** "Resume meeting": records a new session appended to this meeting. */
export function ResumeMeetingButton({
  meetingId,
  meetingTitle,
  busyReason,
}: {
  meetingId: string;
  meetingTitle: string;
  busyReason: string | null;
}) {
  const { t } = useTranslation();
  const router = useRouter();
  const { isRecording } = useRecordingState();
  const reason = isRecording ? t('fork.resume.recordingInProgress') : busyReason;

  const resume = async () => {
    try {
      await checkResume(meetingId);
    } catch (error) {
      toast.error(String(error));
      return;
    }
    requestResume({ meetingId, title: meetingTitle });
    router.push('/');
  };

  const button = (
    <Button size="sm" variant="outline" className="h-7 gap-1 text-xs" disabled={!!reason} onClick={resume}>
      <PlayCircle className="h-3.5 w-3.5" />
      {t('fork.resume.button')}
    </Button>
  );
  if (!reason) return button;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span>{button}</span>
      </TooltipTrigger>
      <TooltipContent>
        <p>{reason}</p>
      </TooltipContent>
    </Tooltip>
  );
}
```

- [ ] **Step 2 : `MeetingCloudBar.tsx`**

Ajouter les props et le bouton en tête du `<span className="ml-auto ...">` :

```tsx
export function MeetingCloudBar({
  meetingId,
  meetingTitle,
  summaryBusy,
  pipeline,
}: {
  meetingId: string;
  meetingTitle: string;
  summaryBusy: boolean;
  pipeline: Pipeline;
}) {
  // ... inchangé ...
        <span className="ml-auto inline-flex items-center gap-1">
          <ResumeMeetingButton
            meetingId={meetingId}
            meetingTitle={meetingTitle}
            busyReason={busy || pipeline.autoRunning ? t('fork.resume.pipelineBusy') : summaryBusy ? t('fork.resume.summaryBusy') : null}
          />
          <ExportButton ... />
```

`pipeline.autoRunning` n'est utilisable que si le hook l'expose. Le vérifier dans le `return` de `usePostMeetingPipeline` ; sinon, n'utiliser que `busy`.

`page-content.tsx:257` :

```tsx
              topBar={
                <MeetingCloudBar
                  meetingId={meeting.id}
                  meetingTitle={meeting.title}
                  summaryBusy={['processing', 'summarizing', 'regenerating'].includes(summaryGeneration.summaryStatus)}
                  pipeline={pipeline}
                />
              }
```

- [ ] **Step 3 : `page.tsx` et `page-content.tsx`, régénération après reprise**

`page.tsx` :
- ligne 74 : `if (source !== 'recording' && source !== 'resume') {` ;
- ligne 282 : `fromRecording={source === 'recording' || source === 'resume'}` ;
- ajouter la prop `resumed={source === 'resume'}`.

`page-content.tsx` :
- prop `resumed = false` (type `resumed?: boolean`) ;
- importer `canAutoGenerate` depuis `@/lib/fork/resume` ;
- dans l'effet d'auto-génération, remplacer `|| summaryGeneration.summaryStatus !== 'idle'` par `|| !canAutoGenerate(summaryGeneration.summaryStatus, resumed)` ;
- remplacer l'appel final par :

```tsx
    void meetingContext.flush().then(() =>
      resumed ? summaryGeneration.handleRegenerateSummary() : summaryGeneration.handleGenerateSummary(''),
    );
```

Ajouter `resumed` et `summaryGeneration.handleRegenerateSummary` aux dépendances de l'effet.

- [ ] **Step 4 : `MeetingSetupContext.tsx`, mode reprise**

- types :

```tsx
import type { ResumeRequest } from '@/lib/fork/resume';
import { getMeetingContext } from '@/services/forkService';

export interface SetupOptions {
  /** Resume this stopped meeting: its context is pre-filled, its title kept */
  resume?: ResumeRequest;
}
// dans MeetingSetupContextValue :
  requestSetup: (options?: SetupOptions) => Promise<MeetingSetupResult | null>;
```

- état : `const [resumeOf, setResumeOf] = useState<ResumeRequest | null>(null);`
- `requestSetup = useCallback(async (options?: SetupOptions) => { ... })`. Après le calcul de `captureDefault` et `screenMonitor`, remplacer le `setDraft(...)` par :

```tsx
    setResumeOf(options?.resume ?? null);
    let initial: MeetingContext = {
      meetingType: 'visio',
      participants: selfName ? [{ name: selfName, isSelf: true }] : [],
      captureScreen: captureDefault,
    };
    if (options?.resume) {
      try {
        const saved = await getMeetingContext(options.resume.meetingId);
        initial = { ...saved, captureScreen: saved.captureScreen ?? captureDefault };
      } catch (error) {
        console.warn('Could not load the context of the resumed meeting:', error);
      }
    }
    setTitle(options?.resume?.title ?? '');
    setDraft(initial);
```

- `finishSetup` : ajouter `setResumeOf(null);` après `setMode(null);`.
- `startWith` : `title: resumeOf ? resumeOf.title : title.trim() || null,`.
- en-tête de la fenêtre :

```tsx
            <DialogTitle>
              {mode === 'live' ? t('fork.setup.liveTitle') : resumeOf ? t('fork.resume.dialogTitle', { title: resumeOf.title }) : t('fork.setup.title')}
            </DialogTitle>
            <DialogDescription>
              {mode === 'live' ? t('fork.setup.liveDescription') : resumeOf ? t('fork.resume.dialogWarning') : t('fork.setup.description')}
            </DialogDescription>
```

- champ titre : `{mode === 'setup' && !resumeOf && ( ... )}`.
- pied de la fenêtre en mode setup : masquer « Démarrer sans contexte » quand `resumeOf`. Le bouton principal devient `{resumeOf ? t('fork.resume.start') : t('fork.setup.start')}`.

- [ ] **Step 5 : `useRecordingStart.ts`, chemin auto-start**

- imports :

```ts
import { cancelResume, prepareResume, setLiveMeetingContext } from '@/services/forkService';
import { takeResumeRequest, type ResumeRequest } from '@/lib/fork/resume';
import { forgetPostMeetingRun } from '@/hooks/fork/usePostMeetingPipeline';
```

- `prepareMeeting` :

```ts
  const prepareMeeting = useCallback(async (resume?: ResumeRequest | null): Promise<string | null> => {
    const setup = await requestSetup(resume ? { resume } : undefined);
    // ... inchangé ...
  }, [requestSetup, generateMeetingTitle]);
```

- dans `checkAutoStartRecording`, juste après `sessionStorage.removeItem('autoStartRecording');` :

```ts
          // Fork: "Resume meeting" from a meeting page arrives through the auto-start
          const resume = takeResumeRequest();
```

- remplacer `const generatedMeetingTitle = await prepareMeeting();` par `await prepareMeeting(resume);`. Après `setStatus(RecordingStatus.STARTING, ...)` :

```ts
            if (resume) {
              await prepareResume(resume.meetingId);
              forgetPostMeetingRun(resume.meetingId);
            }
```

- au début du `catch (error)` de ce chemin :

```ts
            if (resume) await cancelResume().catch(() => undefined);
```

- [ ] **Step 6 : `ResumeBanner.tsx` et `app/page.tsx`**

```tsx
'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { getActiveResume, type ResumeTarget } from '@/services/forkService';
import { formatElapsed } from '@/lib/fork/resume';

/** Above the recording controls while a stopped meeting is being resumed. */
export function ResumeBanner() {
  const { t } = useTranslation();
  const [target, setTarget] = useState<ResumeTarget | null>(null);

  useEffect(() => {
    const refresh = () => getActiveResume().then(setTarget).catch(() => setTarget(null));
    refresh();
    const started = listen('recording-started', refresh);
    const stopped = listen('recording-stopped', () => setTarget(null));
    const failed = listen<{ message: string }>('resume-merge-failed', event =>
      toast.error(t('fork.resume.mergeFailed'), { description: event.payload.message, duration: 15000 }),
    );
    return () => {
      [started, stopped, failed].forEach(p => p.then(fn => fn()));
    };
  }, [t]);

  if (!target) return null;
  return (
    <div className="pointer-events-none fixed bottom-32 left-0 right-0 z-10 flex justify-center">
      <div className="rounded-full bg-amber-50 px-4 py-1 text-xs text-amber-800 shadow">
        {t('fork.resume.banner', { title: target.title, time: formatElapsed(target.offsetSeconds) })}
      </div>
    </div>
  );
}
```

Dans `app/page.tsx` :
- placer `<ResumeBanner />` juste avant `<div className="flex flex-1 overflow-hidden">`, toujours monté (l'événement d'échec du recollage arrive pendant l'arrêt) ;
- ajouter l'import `import { ResumeBanner } from '@/components/fork/ResumeBanner';`.

- [ ] **Step 7 : `useRecordingStop.ts`**

Après `const meetingId = responseData.meeting_id;` et sa vérification :

```ts
          // Fork: a resumed meeting keeps its summary language and regenerates its summary
          const resumed = responseData.resumed === true;
```

- encadrer la détection de langue (`let shouldDetectSummaryLanguage ... if (shouldDetectSummaryLanguage) { ... }`) par `if (!resumed) { ... }` ;
- remplacer la navigation par :

```ts
            router.push(`/meeting-details?id=${meetingId}&source=${sourceAfterSave(responseData)}`);
```

- ajouter l'import `import { sourceAfterSave } from '@/lib/fork/resume';`.

- [ ] **Step 8 : vérifications**

Run : `cd frontend && npx tsc --noEmit && bun test && node scripts/check-i18n-keys.mjs`

Expected : types propres, tous les tests bun verts, clés équilibrées.

- [ ] **Step 9 : commit** `feat(fork): resume button, resume dialog, banner and regenerated summary`

---

### Task 7 : livraison perso.8 et essai réel

**Files :** `frontend/src-tauri/tauri.conf.json` (version), `FORK.md`, `installateur/LISEZMOI.md` si la version y figure, mémoire du projet.

- [ ] **Step 1 :** version `0.4.1-perso.8`.

Deux lignes dans le tableau de `FORK.md` :
- « Reprendre une réunion » : barre de la réunion, `.reprise-N/`, chaîne refaite ;
- « Date et heure dans le compte rendu » : en-tête fixe, IA informée, date de début.

Une ligne dans « Fichiers ajoutés dans le dossier d'une réunion » : `metadata.json` → `sessions`, et `.reprise-N/` temporaire.

- [ ] **Step 2 :** tests complets.
  - Rust : `cargo test --lib fork:: summary::`.
  - Interface : `bun test`, `tsc`, contrôle i18n.

- [ ] **Step 3 :** installateur.
  - Lancer `frontend\scripts\fork-build-installer.cmd`.
  - Avant l'installation, vérifier qu'aucun enregistrement n'est en cours (pas de `.checkpoints` modifié depuis moins de 2 min sous `%USERPROFILE%\Music\meetily-recordings`) et qu'aucun compte rendu n'est `processing`.
  - Installer `installateur\Meetily-perso-0.4.1-perso.8-setup.exe`.

- [ ] **Step 4 :** essai réel, sur une réunion de test et des données synthétiques.
  1. Réunion « Test reprise », capture d'écran cochée. Jouer un dialogue synthétique d'environ 60 s par les haut-parleurs (fichier TTS), puis arrêter.
  2. Attendre la fin de la chaîne : intervenants, compte rendu. Vérifier l'en-tête « … · 1 min ».
  3. « Reprendre la réunion », jouer 60 s d'un autre dialogue, puis arrêter.
  4. Points à contrôler :
     - `audio.mp4` dure environ 2 min ;
     - `.reprise-2` a disparu ;
     - `metadata.json` contient 2 sessions ;
     - la transcription est continue, sans saut de temps ;
     - les captures suivent la numérotation avec `t > D` ;
     - le compte rendu est régénéré et son en-tête mentionne « reprise » ;
     - la page Notion est mise à jour.
  5. Supprimer la réunion de test (application, puis Notion).

- [ ] **Step 5 :** commit `Meetily perso 0.4.1-perso.8: resume a stopped meeting, date and time in summaries`, puis push sur `fork/main`. Mettre à jour la mémoire `meetily-fork-project.md`.

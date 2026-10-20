// fork/meeting_context.rs
//
// Meeting context + participants: entered before/during/after a meeting, persisted
// per meeting, injected into summary prompts, used to name diarized speakers and
// as vocabulary hints for cloud transcription.

use crate::state::AppState;
use chrono::Utc;
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub is_self: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingContext {
    #[serde(default)]
    pub context_text: Option<String>,
    #[serde(default)]
    pub glossary: Option<String>,
    /// 'presentiel' | 'visio'
    #[serde(default)]
    pub meeting_type: Option<String>,
    #[serde(default)]
    pub extra_instructions: Option<String>,
    #[serde(default)]
    pub participants: Vec<Participant>,
    /// Live-only flag (screen capture requested for this recording); not persisted.
    #[serde(default)]
    pub capture_screen: Option<bool>,
}

impl MeetingContext {
    pub fn is_empty(&self) -> bool {
        non_empty(&self.meeting_type).is_none()
            && non_empty(&self.context_text).is_none()
            && non_empty(&self.glossary).is_none()
            && non_empty(&self.extra_instructions).is_none()
            && self.participants.is_empty()
    }

    pub fn is_in_person(&self) -> bool {
        self.meeting_type.as_deref() == Some("presentiel")
    }

    /// Proper nouns worth biasing transcription towards (names, companies, glossary terms).
    pub fn vocabulary(&self) -> Vec<String> {
        let mut terms: Vec<String> = Vec::new();
        for p in &self.participants {
            terms.push(p.name.trim().to_string());
            if let Some(company) = non_empty(&p.company) {
                terms.push(company.to_string());
            }
        }
        if let Some(glossary) = non_empty(&self.glossary) {
            terms.extend(
                glossary
                    .split(|c| c == ',' || c == ';' || c == '\n')
                    .map(|t| t.trim().to_string()),
            );
        }
        terms.retain(|t| !t.is_empty());
        terms.dedup();
        terms.truncate(50);
        terms
    }

    /// Block injected into LLM prompts. Instructions found inside are data, not orders:
    /// the summary system prompt says so explicitly.
    pub fn render_for_prompt(&self) -> String {
        let mut out = String::new();
        if let Some(kind) = non_empty(&self.meeting_type) {
            let label = if kind == "presentiel" { "in person (single room microphone)" } else { "video call" };
            out.push_str(&format!("Meeting format: {}\n", label));
        }
        if !self.participants.is_empty() {
            out.push_str("Participants:\n");
            for p in &self.participants {
                let mut line = format!("- {}", p.name.trim());
                let details: Vec<&str> = [non_empty(&p.role), non_empty(&p.company)]
                    .into_iter()
                    .flatten()
                    .collect();
                if !details.is_empty() {
                    line.push_str(&format!(" ({})", details.join(", ")));
                }
                if p.is_self {
                    line.push_str(" [the person who recorded the meeting]");
                }
                out.push_str(&line);
                out.push('\n');
            }
        }
        if let Some(text) = non_empty(&self.context_text) {
            out.push_str(&format!("Background:\n{}\n", text));
        }
        if let Some(glossary) = non_empty(&self.glossary) {
            out.push_str(&format!("Glossary / spelling of names and terms:\n{}\n", glossary));
        }
        out.trim_end().to_string()
    }
}

pub fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub role: Option<String>,
    pub company: Option<String>,
}

pub struct MeetingContextRepository;

impl MeetingContextRepository {
    pub async fn get(pool: &SqlitePool, meeting_id: &str) -> Result<MeetingContext, sqlx::Error> {
        let row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>, Option<String>)>(
            "SELECT context_text, glossary, meeting_type, extra_instructions FROM meeting_context WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?;

        let participants = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, i64)>(
            "SELECT id, name, role, company, is_self FROM meeting_participants
             WHERE meeting_id = ? ORDER BY sort_order ASC, created_at ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, name, role, company, is_self)| Participant {
            id: Some(id),
            name,
            role,
            company,
            is_self: is_self != 0,
        })
        .collect();

        let (context_text, glossary, meeting_type, extra_instructions) = row.unwrap_or_default();
        Ok(MeetingContext {
            context_text,
            glossary,
            meeting_type,
            extra_instructions,
            participants,
            capture_screen: None,
        })
    }

    /// Upsert the context and replace the participant list; also feeds the contact book.
    pub async fn save(pool: &SqlitePool, meeting_id: &str, ctx: &MeetingContext) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;

        sqlx::query(
            "INSERT INTO meeting_context (meeting_id, context_text, glossary, meeting_type, extra_instructions, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(meeting_id) DO UPDATE SET
                context_text = excluded.context_text,
                glossary = excluded.glossary,
                meeting_type = excluded.meeting_type,
                extra_instructions = excluded.extra_instructions,
                updated_at = excluded.updated_at",
        )
        .bind(meeting_id)
        .bind(&ctx.context_text)
        .bind(&ctx.glossary)
        .bind(&ctx.meeting_type)
        .bind(&ctx.extra_instructions)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM meeting_participants WHERE meeting_id = ?")
            .bind(meeting_id)
            .execute(&mut *tx)
            .await?;

        for (index, p) in ctx.participants.iter().enumerate() {
            let name = p.name.trim();
            if name.is_empty() {
                continue;
            }
            let id = p.id.clone().unwrap_or_else(|| format!("participant-{}", Uuid::new_v4()));
            sqlx::query(
                "INSERT INTO meeting_participants (id, meeting_id, name, role, company, is_self, sort_order, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(meeting_id)
            .bind(name)
            .bind(non_empty(&p.role))
            .bind(non_empty(&p.company))
            .bind(p.is_self as i64)
            .bind(index as i64)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO contacts (id, name, role, company, last_used_at) VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(name, IFNULL(company, '')) DO UPDATE SET
                    role = COALESCE(excluded.role, contacts.role),
                    last_used_at = excluded.last_used_at",
            )
            .bind(format!("contact-{}", Uuid::new_v4()))
            .bind(name)
            .bind(non_empty(&p.role))
            .bind(non_empty(&p.company))
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await
    }

    pub async fn search_contacts(pool: &SqlitePool, query: &str) -> Result<Vec<Contact>, sqlx::Error> {
        let pattern = format!("%{}%", query.trim().to_lowercase());
        sqlx::query_as::<_, Contact>(
            "SELECT id, name, role, company FROM contacts
             WHERE LOWER(name) LIKE ? OR LOWER(IFNULL(company, '')) LIKE ?
             ORDER BY last_used_at DESC LIMIT 12",
        )
        .bind(&pattern)
        .bind(&pattern)
        .fetch_all(pool)
        .await
    }
}

/// Context of the meeting currently being recorded (before it has a meeting_id).
static LIVE_MEETING_CONTEXT: Lazy<RwLock<Option<MeetingContext>>> = Lazy::new(|| RwLock::new(None));

pub fn live_context() -> Option<MeetingContext> {
    LIVE_MEETING_CONTEXT.read().ok().and_then(|guard| guard.clone())
}

pub fn set_live_context(ctx: Option<MeetingContext>) {
    if let Ok(mut guard) = LIVE_MEETING_CONTEXT.write() {
        *guard = ctx;
    }
}

#[tauri::command]
pub async fn api_get_meeting_context(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingContext, String> {
    MeetingContextRepository::get(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn api_save_meeting_context(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    context: MeetingContext,
) -> Result<(), String> {
    info!("Saving context for meeting {} ({} participants)", meeting_id, context.participants.len());
    MeetingContextRepository::save(state.db_manager.pool(), &meeting_id, &context)
        .await
        .map_err(|e| e.to_string())
}

/// Set (or clear with `null`) the context of the recording in progress. Also written to
/// `context.json` in the meeting folder so a crash does not lose it.
#[tauri::command]
pub async fn api_set_live_meeting_context(context: Option<MeetingContext>) -> Result<(), String> {
    if let Some(ref ctx) = context {
        if let Ok(Some(folder)) = crate::audio::recording_commands::get_meeting_folder_path().await {
            let path = std::path::Path::new(&folder).join("context.json");
            match serde_json::to_string_pretty(ctx) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&path, json) {
                        warn!("Could not write {}: {}", path.display(), e);
                    }
                }
                Err(e) => warn!("Could not serialize live context: {}", e),
            }
        }
    }
    set_live_context(context);
    Ok(())
}

#[tauri::command]
pub async fn api_get_live_meeting_context() -> Result<Option<MeetingContext>, String> {
    Ok(live_context())
}

#[tauri::command]
pub async fn api_search_contacts(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<Vec<Contact>, String> {
    MeetingContextRepository::search_contacts(state.db_manager.pool(), &query)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MeetingContext {
        MeetingContext {
            context_text: Some("Point mensuel migration SQL".into()),
            glossary: Some("WebAdmin, IIS ; SIRIS".into()),
            meeting_type: Some("visio".into()),
            extra_instructions: None,
            participants: vec![
                Participant { name: "Loïc".into(), company: Some("RISS".into()), is_self: true, ..Default::default() },
                Participant { name: "Marie Tremblay".into(), role: Some("DSI".into()), company: Some("Acme".into()), ..Default::default() },
            ],
            capture_screen: None,
        }
    }

    #[test]
    fn vocabulary_collects_names_companies_and_glossary() {
        let vocab = sample().vocabulary();
        assert!(vocab.contains(&"Marie Tremblay".to_string()));
        assert!(vocab.contains(&"Acme".to_string()));
        assert!(vocab.contains(&"WebAdmin".to_string()));
        assert!(vocab.contains(&"SIRIS".to_string()));
    }

    #[test]
    fn prompt_block_lists_participants_with_details() {
        let block = sample().render_for_prompt();
        assert!(block.contains("- Marie Tremblay (DSI, Acme)"));
        assert!(block.contains("[the person who recorded the meeting]"));
        assert!(block.contains("Background:\nPoint mensuel migration SQL"));
    }

    #[test]
    fn empty_context_is_detected() {
        assert!(MeetingContext::default().is_empty());
        assert!(!sample().is_empty());
    }
}

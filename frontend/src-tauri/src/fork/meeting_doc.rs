// fork/meeting_doc.rs
//
// Everything about one meeting in a single structure (title, date, tags, context,
// summary, transcript with speaker names, cloud cost), rendered as Markdown for the
// export, the Notion page and the AI search digests.

use super::i18n::is_french;
use super::meeting_context::{MeetingContext, MeetingContextRepository};
use super::speakers::{MeetingSpeaker, SpeakersRepository};
use super::tags::{Tag, TagsRepository};
use chrono::{DateTime, Local};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TranscriptLine {
    pub start: Option<f64>,
    /// Display name of the speaker (None when unknown / mixed)
    pub speaker: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct MeetingDoc {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub folder: Option<PathBuf>,
    pub duration_secs: Option<f64>,
    pub tags: Vec<Tag>,
    pub context: MeetingContext,
    pub summary_markdown: Option<String>,
    pub lines: Vec<TranscriptLine>,
    pub cloud_cost: f64,
}

/// Human label of a raw speaker key ('mic', 'system', 'speaker_N'), as in the UI.
pub fn speaker_label(key: Option<&str>, speakers: &HashMap<String, MeetingSpeaker>) -> Option<String> {
    let key = key?;
    if key == "mixed" {
        return None;
    }
    let known = speakers.get(key);
    if let Some(name) = known.and_then(|s| s.display_name.as_deref()).filter(|n| !n.trim().is_empty()) {
        return Some(name.trim().to_string());
    }
    let fr = is_french();
    if key == "mic" || known.map(|s| s.source == "mic-anchor").unwrap_or(false) {
        return Some(if fr { "Moi" } else { "Me" }.to_string());
    }
    if key == "system" {
        return Some(if fr { "Interlocuteurs" } else { "Remote side" }.to_string());
    }
    if let Some(n) = key.strip_prefix("speaker_").and_then(|n| n.parse::<usize>().ok()) {
        return Some(if fr { format!("Intervenant {}", n + 1) } else { format!("Speaker {}", n + 1) });
    }
    Some(key.to_string())
}

pub fn mm_ss(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    if total >= 3600 {
        format!("{:02}:{:02}:{:02}", total / 3600, (total / 60) % 60, total % 60)
    } else {
        format!("{:02}:{:02}", total / 60, total % 60)
    }
}

/// Extract the Markdown of a stored summary (summary_processes.result JSON).
pub fn summary_markdown_from_result(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    value
        .get("markdown")
        .and_then(|m| m.as_str())
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
}

pub async fn load(pool: &SqlitePool, meeting_id: &str) -> Result<MeetingDoc, String> {
    let (title, created_at, folder): (String, String, Option<String>) =
        sqlx::query_as("SELECT title, created_at, folder_path FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Réunion introuvable".to_string())?;

    let speakers: HashMap<String, MeetingSpeaker> = SpeakersRepository::list(pool, meeting_id)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| (s.speaker_key.clone(), s))
        .collect();

    let rows: Vec<(String, Option<f64>, Option<f64>, Option<String>)> = sqlx::query_as(
        "SELECT transcript, audio_start_time, audio_end_time, speaker FROM transcripts
         WHERE meeting_id = ? ORDER BY audio_start_time ASC",
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let duration_secs = rows.iter().filter_map(|r| r.2).fold(None, |acc: Option<f64>, e| Some(acc.map_or(e, |a| a.max(e))));
    let lines = rows
        .into_iter()
        .filter(|r| !r.0.trim().is_empty())
        .map(|(text, start, _, speaker)| TranscriptLine {
            start,
            speaker: speaker_label(speaker.as_deref(), &speakers),
            text: text.trim().to_string(),
        })
        .collect();

    let summary_markdown = sqlx::query_as::<_, (Option<String>,)>("SELECT result FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .and_then(|(r,)| r)
        .and_then(|raw| summary_markdown_from_result(&raw));

    let cloud_cost: Option<f64> = sqlx::query_as::<_, (Option<f64>,)>("SELECT SUM(cost_usd) FROM meeting_usage WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_one(pool)
        .await
        .map(|(c,)| c)
        .unwrap_or(None);

    Ok(MeetingDoc {
        id: meeting_id.to_string(),
        title,
        created_at,
        folder: folder.map(PathBuf::from),
        duration_secs,
        tags: TagsRepository::for_meeting(pool, meeting_id).await.map_err(|e| e.to_string())?,
        context: MeetingContextRepository::get(pool, meeting_id).await.map_err(|e| e.to_string())?,
        summary_markdown,
        lines,
        cloud_cost: cloud_cost.unwrap_or(0.0),
    })
}

impl MeetingDoc {
    pub fn local_date(&self) -> Option<DateTime<Local>> {
        DateTime::parse_from_rfc3339(&self.created_at).ok().map(|d| d.with_timezone(&Local))
    }

    pub fn date_label(&self) -> String {
        self.local_date()
            .map(|d| d.format(if is_french() { "%d/%m/%Y %H:%M" } else { "%Y-%m-%d %H:%M" }).to_string())
            .unwrap_or_else(|| self.created_at.clone())
    }

    pub fn participants_label(&self) -> String {
        self.context
            .participants
            .iter()
            .map(|p| {
                let details: Vec<&str> = [p.role.as_deref(), p.company.as_deref()]
                    .into_iter()
                    .flatten()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                if details.is_empty() { p.name.trim().to_string() } else { format!("{} ({})", p.name.trim(), details.join(", ")) }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Transcript as Markdown; the speaker name is written when it changes.
    pub fn transcript_markdown(&self) -> String {
        let fr = is_french();
        let mut out = format!("# {} — {}\n\n*{}*\n\n", self.title, if fr { "Transcription" } else { "Transcript" }, self.date_label());
        let mut last: Option<&str> = None;
        for line in &self.lines {
            let time = line.start.map(|s| format!("[{}] ", mm_ss(s))).unwrap_or_default();
            match line.speaker.as_deref() {
                Some(name) if Some(name) != last => out.push_str(&format!("{}**{}** : {}\n\n", time, name, line.text)),
                _ => out.push_str(&format!("{}{}\n\n", time, line.text)),
            }
            last = line.speaker.as_deref().or(last);
        }
        out
    }

    pub fn summary_document(&self) -> Option<String> {
        let summary = self.summary_markdown.as_ref()?;
        Some(format!("# {}\n\n*{}*\n\n{}\n", self.title, self.date_label(), summary))
    }

    pub fn context_markdown(&self) -> Option<String> {
        if self.context.is_empty() && self.tags.is_empty() {
            return None;
        }
        let fr = is_french();
        let mut out = format!("# {} — {}\n\n", self.title, if fr { "Contexte" } else { "Context" });
        if !self.tags.is_empty() {
            let names: Vec<&str> = self.tags.iter().map(|t| t.name.as_str()).collect();
            out.push_str(&format!("**Tags** : {}\n\n", names.join(", ")));
        }
        if !self.context.participants.is_empty() {
            out.push_str(&format!("**{}** : {}\n\n", if fr { "Participants" } else { "Participants" }, self.participants_label()));
        }
        if let Some(text) = super::meeting_context::non_empty(&self.context.context_text) {
            out.push_str(&format!("## {}\n\n{}\n\n", if fr { "Contexte" } else { "Background" }, text));
        }
        if let Some(glossary) = super::meeting_context::non_empty(&self.context.glossary) {
            out.push_str(&format!("## {}\n\n{}\n\n", if fr { "Glossaire" } else { "Glossary" }, glossary));
        }
        if let Some(instructions) = super::meeting_context::non_empty(&self.context.extra_instructions) {
            out.push_str(&format!("## {}\n\n{}\n\n", if fr { "Consignes du compte rendu" } else { "Summary instructions" }, instructions));
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speaker(key: &str, name: Option<&str>, source: &str) -> (String, MeetingSpeaker) {
        (
            key.to_string(),
            MeetingSpeaker {
                speaker_key: key.into(),
                display_name: name.map(String::from),
                participant_id: None,
                source: source.into(),
                confidence: None,
            },
        )
    }

    #[test]
    fn labels_follow_names_then_defaults() {
        let map: HashMap<_, _> = [speaker("speaker_1", Some("Marie"), "auto"), speaker("speaker_0", None, "mic-anchor")].into_iter().collect();
        assert_eq!(speaker_label(Some("speaker_1"), &map).as_deref(), Some("Marie"));
        assert_eq!(speaker_label(Some("speaker_0"), &map).as_deref(), Some("Moi"));
        assert_eq!(speaker_label(Some("speaker_3"), &map).as_deref(), Some("Intervenant 4"));
        assert_eq!(speaker_label(Some("mixed"), &map), None);
        assert_eq!(speaker_label(None, &map), None);
    }

    #[test]
    fn transcript_markdown_writes_names_on_change() {
        let doc = MeetingDoc {
            id: "m".into(),
            title: "Point Acme".into(),
            created_at: "2026-10-21T14:00:00+00:00".into(),
            folder: None,
            duration_secs: Some(70.0),
            tags: vec![],
            context: Default::default(),
            summary_markdown: Some("**Résumé**\n\nOK".into()),
            lines: vec![
                TranscriptLine { start: Some(5.0), speaker: Some("Loïc".into()), text: "Bonjour".into() },
                TranscriptLine { start: Some(9.0), speaker: Some("Loïc".into()), text: "On commence".into() },
                TranscriptLine { start: Some(65.0), speaker: Some("Marie".into()), text: "D'accord".into() },
            ],
            cloud_cost: 0.0,
        };
        let md = doc.transcript_markdown();
        assert!(md.contains("[00:05] **Loïc** : Bonjour"));
        assert!(md.contains("[00:09] On commence"));
        assert!(md.contains("[01:05] **Marie** : D'accord"));
        assert!(doc.summary_document().unwrap().starts_with("# Point Acme"));
        assert!(doc.context_markdown().is_none());
    }

    #[test]
    fn reads_markdown_from_summary_result() {
        assert_eq!(summary_markdown_from_result(r#"{"markdown":"  # T\nx  ","english_cache":{}}"#).as_deref(), Some("# T\nx"));
        assert_eq!(summary_markdown_from_result(r#"{"MeetingName":"x"}"#), None);
        assert_eq!(summary_markdown_from_result("not json"), None);
    }
}

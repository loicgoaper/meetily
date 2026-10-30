// fork/summary_context.rs
//
// Builds the extra input of a summary: the persisted meeting context, what was shown
// on screen (vision notes) and the user's own instructions, as tagged blocks that
// summary/processor.rs passes to the model next to the transcript.

use super::meeting_context::{non_empty, MeetingContextRepository};
use log::warn;
use sqlx::SqlitePool;

/// `screen_budget`: characters of screen notes the model can take (see vision::PROMPT_CHARS_*).
/// `summary_language`: language of the summary, for the date written in the prompt.
pub async fn build(
    pool: &SqlitePool,
    meeting_id: &str,
    user_instructions: Option<&str>,
    screen_budget: usize,
    summary_language: Option<&str>,
) -> String {
    let context = match MeetingContextRepository::get(pool, meeting_id).await {
        Ok(ctx) => ctx,
        Err(e) => {
            warn!("Could not load meeting context for {}: {}", meeting_id, e);
            Default::default()
        }
    };

    let mut blocks: Vec<String> = Vec::new();
    // Date, time and sessions first, so dates and deadlines can be written out
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

    if let Ok(folder) = super::diarization::meeting_folder(pool, meeting_id).await {
        if let Some(notes) = super::vision::notes_for_prompt(&folder, screen_budget) {
            blocks.push(format!("<screen_content>\n{}\n</screen_content>", notes));
        }
    }

    let instructions = user_instructions
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| non_empty(&context.extra_instructions).map(str::to_string));
    if let Some(text) = instructions {
        blocks.push(format!("<user_instructions>\n{}\n</user_instructions>", text));
    }

    blocks.join("\n\n")
}

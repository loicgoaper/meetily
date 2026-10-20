// fork/summary_context.rs
//
// Builds the extra input of a summary: the persisted meeting context, what was shown
// on screen (vision notes) and the user's own instructions, as tagged blocks that
// summary/processor.rs passes to the model next to the transcript.

use super::meeting_context::{non_empty, MeetingContextRepository};
use log::warn;
use sqlx::SqlitePool;

pub async fn build(pool: &SqlitePool, meeting_id: &str, user_instructions: Option<&str>) -> String {
    let context = match MeetingContextRepository::get(pool, meeting_id).await {
        Ok(ctx) => ctx,
        Err(e) => {
            warn!("Could not load meeting context for {}: {}", meeting_id, e);
            Default::default()
        }
    };

    let mut blocks: Vec<String> = Vec::new();
    let rendered = context.render_for_prompt();
    if !rendered.is_empty() {
        blocks.push(format!("<meeting_context>\n{}\n</meeting_context>", rendered));
    }

    if let Ok(folder) = super::diarization::meeting_folder(pool, meeting_id).await {
        if let Some(notes) = super::vision::notes_for_prompt(&folder) {
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

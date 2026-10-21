// fork/mod.rs
//
// Meetily perso: everything added by the fork lives here, so upstream files only
// receive small hooks and future merges stay manageable.

pub mod ai_search;
pub mod cloud_chunker;
pub mod diarization;
pub mod export;
pub mod i18n;
pub mod meeting_context;
pub mod meeting_doc;
pub mod notion;
pub mod notion_md;
pub mod openrouter_chat;
pub mod openrouter_stt;
pub mod preferences;
pub mod screen_capture;
pub mod source_timeline;
pub mod speakers;
pub mod stt_provider;
pub mod summary_context;
pub mod tag_suggest;
pub mod tags;
pub mod templates_admin;
pub mod usage;
pub mod vision;
pub mod wav;

#[cfg(test)]
mod db_tests;
#[cfg(test)]
mod live_tests;

use log::{info, warn};
use sqlx::SqlitePool;

/// Called right after a recorded meeting is saved (api_save_transcript): stores the
/// context entered during the recording and the live cloud STT cost, then clears the
/// live state. Errors are logged, never returned: the transcript is already safe.
pub async fn persist_new_meeting_extras(
    pool: &SqlitePool,
    meeting_id: &str,
    context: Option<&meeting_context::MeetingContext>,
) {
    let context = context.cloned().or_else(meeting_context::live_context);
    if let Some(ctx) = context.filter(|c| !c.is_empty()) {
        match meeting_context::MeetingContextRepository::save(pool, meeting_id, &ctx).await {
            Ok(()) => info!("Saved meeting context ({} participants) for {}", ctx.participants.len(), meeting_id),
            Err(e) => warn!("Failed to save meeting context for {}: {}", meeting_id, e),
        }
    }
    meeting_context::set_live_context(None);

    let live = usage::take_live();
    if live.requests > 0 {
        if let Err(e) = usage::record(
            pool,
            meeting_id,
            "stt_live",
            live.model.as_deref(),
            Some(live.audio_seconds),
            Some(live.cost_usd),
        )
        .await
        {
            warn!("Failed to record live STT usage for {}: {}", meeting_id, e);
        }
    }
}

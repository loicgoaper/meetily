// fork/preferences.rs
//
// Key/value settings of the fork (cloud models, privacy, vision, UI language...),
// stored in the `app_preferences` table so no upstream table is ever altered.

use crate::state::AppState;
use chrono::Utc;
use log::{info, warn};
use sqlx::SqlitePool;
use std::collections::HashMap;
use tauri::{AppHandle, Manager, Runtime};

pub const STT_LIVE_MODEL: &str = "stt.live_model";
pub const STT_LANGUAGE: &str = "stt.language";
pub const STT_FALLBACK_LOCAL: &str = "stt.fallback_local";
pub const STT_CONCURRENCY: &str = "stt.concurrency";
pub const PRIVACY_ZDR: &str = "privacy.zdr";
pub const DIARIZATION_AUTO: &str = "diarization.auto";
pub const DIARIZATION_MODEL: &str = "diarization.model";
pub const NAMING_MODEL: &str = "naming.model";
pub const VISION_MODEL: &str = "vision.model";
pub const SCREEN_INTERVAL_SECONDS: &str = "screen.interval_seconds";
pub const SCREEN_DEFAULT_ENABLED: &str = "screen.default_enabled";
pub const SUMMARY_DEFAULT_TEMPLATE: &str = "summary.default_template";
pub const UI_LANGUAGE: &str = "ui.language";
pub const TAGS_AUTO_SUGGEST: &str = "tags.auto_suggest";

/// Defaults applied when a key was never saved.
/// STT/diarization defaults come from the 2026-10-20 spike: elevenlabs/scribe-v2 is
/// fast (1-2 s per 10-30 s chunk), good in French, and the only OpenRouter STT model
/// that accepts `diarize`.
pub fn defaults() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        (STT_LIVE_MODEL, "elevenlabs/scribe-v2"),
        (STT_LANGUAGE, "fr"),
        (STT_FALLBACK_LOCAL, "true"),
        (STT_CONCURRENCY, "3"),
        (PRIVACY_ZDR, "false"),
        (DIARIZATION_AUTO, "true"),
        (DIARIZATION_MODEL, "elevenlabs/scribe-v2"),
        (NAMING_MODEL, "google/gemini-3.1-flash-lite"),
        (VISION_MODEL, "google/gemini-3.1-flash-lite"),
        (SCREEN_INTERVAL_SECONDS, "5"),
        (SCREEN_DEFAULT_ENABLED, "false"),
        (SUMMARY_DEFAULT_TEMPLATE, "fr_reunion_standard"),
        (UI_LANGUAGE, "fr"),
        (TAGS_AUTO_SUGGEST, "true"),
    ])
}

pub struct PreferencesRepository;

impl PreferencesRepository {
    pub async fn get_all(pool: &SqlitePool) -> Result<HashMap<String, String>, sqlx::Error> {
        let mut values: HashMap<String, String> = defaults()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let rows = sqlx::query_as::<_, (String, String)>("SELECT key, value FROM app_preferences")
            .fetch_all(pool)
            .await?;
        values.extend(rows);
        Ok(values)
    }

    pub async fn get(pool: &SqlitePool, key: &str) -> String {
        let stored = sqlx::query_as::<_, (String,)>("SELECT value FROM app_preferences WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await;
        match stored {
            Ok(Some((value,))) => value,
            Ok(None) => defaults().get(key).copied().unwrap_or_default().to_string(),
            Err(e) => {
                warn!("Failed to read preference '{}': {}", key, e);
                defaults().get(key).copied().unwrap_or_default().to_string()
            }
        }
    }

    pub async fn set_many(pool: &SqlitePool, values: &HashMap<String, String>) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        for (key, value) in values {
            sqlx::query(
                "INSERT INTO app_preferences (key, value, updated_at) VALUES (?, ?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            )
            .bind(key)
            .bind(value)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }
}

/// Read one preference from anywhere that has an AppHandle.
pub async fn get_pref<R: Runtime>(app: &AppHandle<R>, key: &str) -> String {
    let state = app.state::<AppState>();
    PreferencesRepository::get(state.db_manager.pool(), key).await
}

pub async fn get_pref_bool<R: Runtime>(app: &AppHandle<R>, key: &str) -> bool {
    get_pref(app, key).await == "true"
}

#[tauri::command]
pub async fn api_get_app_preferences(
    state: tauri::State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    PreferencesRepository::get_all(state.db_manager.pool())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn api_set_app_preferences<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    values: HashMap<String, String>,
) -> Result<(), String> {
    info!("Saving {} app preference(s): {:?}", values.len(), values.keys().collect::<Vec<_>>());
    PreferencesRepository::set_many(state.db_manager.pool(), &values)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(lang) = values.get(UI_LANGUAGE) {
        super::i18n::set_ui_language(&app, lang);
    }
    Ok(())
}

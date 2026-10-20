// fork/stt_provider.rs
//
// Live transcription through OpenRouter, plugged into the upstream
// `TranscriptionProvider` trait. Chunks are regrouped by `cloud_chunker`, sent with up
// to N requests in flight, and fall back to the local Parakeet engine when the cloud
// is unreachable (circuit breaker) or the key is rejected.

use super::openrouter_stt::{self, SttError, SttOptions};
use super::preferences::{self as prefs, PreferencesRepository};
use crate::audio::transcription::provider::{TranscriptResult, TranscriptionError, TranscriptionProvider};
use crate::state::AppState;
use async_trait::async_trait;
use log::{info, warn};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

const CIRCUIT_FAILURES: u32 = 3;
const CIRCUIT_PAUSE: Duration = Duration::from_secs(60);
const BILLING_PAUSE: Duration = Duration::from_secs(300);
/// 0.3 s at 16 kHz: shorter clips are not worth a request.
const MIN_SAMPLES: usize = 4800;

type Notifier = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

pub struct OpenRouterSttProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    language: Option<String>,
    zdr: bool,
    concurrency: usize,
    fallback: Option<Arc<crate::parakeet_engine::ParakeetEngine>>,
    consecutive_failures: AtomicU32,
    paused_until: Mutex<Option<Instant>>,
    auth_failed: AtomicBool,
    notify: Notifier,
}

impl OpenRouterSttProvider {
    fn cloud_available(&self) -> bool {
        if self.auth_failed.load(Ordering::SeqCst) {
            return false;
        }
        let mut paused = self.paused_until.lock().unwrap_or_else(|e| e.into_inner());
        match *paused {
            Some(until) if Instant::now() < until => false,
            Some(_) => {
                *paused = None;
                // Half-open: one more failure trips the breaker again right away
                self.consecutive_failures.store(CIRCUIT_FAILURES - 1, Ordering::SeqCst);
                info!("☁️ Retrying cloud transcription after circuit-breaker pause");
                (self.notify)("cloud-stt-status", serde_json::json!({ "status": "cloud", "model": self.model }));
                true
            }
            None => true,
        }
    }

    fn on_cloud_error(&self, error: &SttError) {
        // 401: the key itself is rejected, nothing will change during this recording
        if matches!(error, SttError::Http { status: 401, .. }) {
            if !self.auth_failed.swap(true, Ordering::SeqCst) {
                warn!("☁️ OpenRouter rejected the key: {}", error);
                (self.notify)(
                    "cloud-stt-status",
                    serde_json::json!({ "status": "auth_error", "message": error.to_string() }),
                );
            }
            return;
        }
        // 402/403: credits or provider refusal; may be fixed mid-meeting, so pause longer then re-probe
        if error.is_auth_or_billing() {
            self.consecutive_failures.store(0, Ordering::SeqCst);
            *self.paused_until.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + BILLING_PAUSE);
            warn!("☁️ OpenRouter refused the request ({}), using local fallback for {:?}", error, BILLING_PAUSE);
            (self.notify)(
                "cloud-stt-status",
                serde_json::json!({ "status": "billing_error", "message": error.to_string() }),
            );
            return;
        }
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        if failures >= CIRCUIT_FAILURES {
            self.consecutive_failures.store(0, Ordering::SeqCst);
            *self.paused_until.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + CIRCUIT_PAUSE);
            warn!("☁️ {} consecutive cloud failures, using local fallback for {:?}", failures, CIRCUIT_PAUSE);
            (self.notify)(
                "cloud-stt-status",
                serde_json::json!({ "status": "fallback", "message": error.to_string() }),
            );
        }
    }

    async fn transcribe_locally(&self, audio: Vec<f32>, cause: &str) -> Result<TranscriptResult, TranscriptionError> {
        match &self.fallback {
            Some(engine) if engine.is_model_loaded().await => {
                super::usage::add_live_fallback();
                let text = engine
                    .transcribe_audio(audio)
                    .await
                    .map_err(|e| TranscriptionError::EngineFailed(e.to_string()))?;
                Ok(TranscriptResult { text: text.trim().to_string(), confidence: None, is_partial: false })
            }
            _ => Err(TranscriptionError::EngineFailed(format!(
                "Cloud transcription unavailable ({}) and no local fallback model is loaded",
                cause
            ))),
        }
    }
}

#[async_trait]
impl TranscriptionProvider for OpenRouterSttProvider {
    async fn transcribe(&self, audio: Vec<f32>, _language: Option<String>) -> Result<TranscriptResult, TranscriptionError> {
        if audio.len() < MIN_SAMPLES {
            return Err(TranscriptionError::AudioTooShort { samples: audio.len(), minimum: MIN_SAMPLES });
        }
        if !self.cloud_available() {
            return self.transcribe_locally(audio, "circuit open").await;
        }

        let seconds = audio.len() as f64 / 16000.0;
        let wav = super::wav::encode_wav_pcm16_mono(&audio, 16000);
        let opts = SttOptions {
            model: self.model.clone(),
            language: self.language.clone(),
            vocabulary: super::meeting_context::live_context().map(|c| c.vocabulary()).unwrap_or_default(),
            zdr: self.zdr,
            ..Default::default()
        };

        match openrouter_stt::transcribe_with_retry(&self.client, &self.api_key, &wav, "wav", &opts).await {
            Ok(response) => {
                self.consecutive_failures.store(0, Ordering::SeqCst);
                let usage = response.usage.unwrap_or_default();
                let totals = super::usage::add_live(usage.seconds.unwrap_or(seconds), usage.cost.unwrap_or(0.0));
                (self.notify)("stt-usage", serde_json::to_value(&totals).unwrap_or_default());
                Ok(TranscriptResult { text: response.text.trim().to_string(), confidence: None, is_partial: false })
            }
            Err(e) => {
                warn!("☁️ Cloud transcription failed for a {:.1}s chunk: {}", seconds, e);
                self.on_cloud_error(&e);
                self.transcribe_locally(audio, &e.to_string()).await
            }
        }
    }

    async fn is_model_loaded(&self) -> bool {
        true
    }

    async fn get_current_model(&self) -> Option<String> {
        Some(self.model.clone())
    }

    fn provider_name(&self) -> &'static str {
        "OpenRouter"
    }

    fn chunking_policy(&self) -> Option<super::cloud_chunker::ChunkingPolicy> {
        Some(super::cloud_chunker::ChunkingPolicy::default())
    }

    fn max_concurrency(&self) -> usize {
        self.concurrency
    }
}

/// Called by `validate_transcription_model_ready` before a recording starts.
pub async fn validate<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    super::openrouter_chat::api_key(app).await?;
    if prefs::get_pref_bool(app, prefs::STT_FALLBACK_LOCAL).await {
        // Best effort: the local model is only a safety net.
        if let Err(e) = crate::parakeet_engine::commands::parakeet_init().await {
            warn!("Local fallback unavailable (Parakeet init failed): {}", e);
        } else if let Err(e) = crate::parakeet_engine::commands::parakeet_validate_model_ready_with_config(app).await {
            warn!("Local fallback unavailable (no Parakeet model): {}", e);
        }
    }
    Ok(())
}

/// Build the live provider from the saved key and fork preferences.
pub async fn build<R: Runtime>(app: &AppHandle<R>, model: &str) -> Result<OpenRouterSttProvider, String> {
    let api_key = super::openrouter_chat::api_key(app).await?;
    let pool = app.state::<AppState>().db_manager.pool().clone();
    let model = if model.trim().is_empty() {
        PreferencesRepository::get(&pool, prefs::STT_LIVE_MODEL).await
    } else {
        model.to_string()
    };
    let language = Some(PreferencesRepository::get(&pool, prefs::STT_LANGUAGE).await).filter(|l| !l.is_empty() && l != "auto");
    let zdr = PreferencesRepository::get(&pool, prefs::PRIVACY_ZDR).await == "true";
    let concurrency = PreferencesRepository::get(&pool, prefs::STT_CONCURRENCY)
        .await
        .parse::<usize>()
        .unwrap_or(3)
        .clamp(1, 6);
    let fallback = if PreferencesRepository::get(&pool, prefs::STT_FALLBACK_LOCAL).await == "true" {
        crate::parakeet_engine::commands::PARAKEET_ENGINE.lock().ok().and_then(|g| g.as_ref().cloned())
    } else {
        None
    };

    super::usage::reset_live(Some(model.clone()));
    let app_for_events = app.clone();
    let notify: Notifier = Arc::new(move |event: &str, payload: serde_json::Value| {
        let _ = app_for_events.emit(event, payload);
    });

    info!(
        "☁️ OpenRouter live STT: model={}, language={:?}, concurrency={}, zdr={}, local fallback={}",
        model, language, concurrency, zdr, fallback.is_some()
    );

    Ok(OpenRouterSttProvider {
        client: openrouter_stt::build_client(Duration::from_secs(45)),
        api_key,
        model,
        language,
        zdr,
        concurrency,
        fallback,
        consecutive_failures: AtomicU32::new(0),
        paused_until: Mutex::new(None),
        auth_failed: AtomicBool::new(false),
        notify,
    })
}

// Readiness commands used by the frontend before starting a recording
// (lib/transcription-model-readiness.ts): a cloud model needs no download, only a key.

#[tauri::command]
pub async fn openrouter_stt_init() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn openrouter_stt_has_key<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    Ok(super::openrouter_chat::api_key(&app).await.is_ok())
}

#[tauri::command]
pub async fn openrouter_stt_models_status() -> Result<Vec<serde_json::Value>, String> {
    Ok(Vec::new())
}

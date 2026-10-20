// fork/openrouter_stt.rs
//
// Client for OpenRouter's speech-to-text endpoint (POST /api/v1/audio/transcriptions),
// shared by live transcription and the end-of-meeting diarization pass.
//
// Findings from the 2026-10-20 spike, which shape this client:
// - `keyterms` is rejected (400) by every STT model, so vocabulary hints are only sent
//   as an OpenAI `prompt` through provider options, for openai/* models.
// - `diarize` is only accepted by elevenlabs/scribe-v2; it returns word-level `speaker`.
// - Each upstream request must finish within ~60 s; a 36 min opus file took 32 s.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const ENDPOINT: &str = "https://openrouter.ai/api/v1/audio/transcriptions";
pub const MODELS_ENDPOINT: &str = "https://openrouter.ai/api/v1/models?output_modalities=transcription";

#[derive(Debug, Clone, Default)]
pub struct SttOptions {
    pub model: String,
    /// ISO-639-1 code; None lets the model detect the language.
    pub language: Option<String>,
    pub diarize: bool,
    /// Request segments/words with timestamps (verbose_json).
    pub verbose: bool,
    /// Names and terms to bias recognition (only used where the provider supports it).
    pub vocabulary: Vec<String>,
    pub zdr: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SttWord {
    #[serde(default)]
    pub word: String,
    // Lenient: some providers emit audio events without timestamps
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub speaker: Option<serde_json::Value>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SttSegment {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub speaker: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SttUsage {
    #[serde(default)]
    pub seconds: Option<f64>,
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SttResponse {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub segments: Vec<SttSegment>,
    #[serde(default)]
    pub words: Vec<SttWord>,
    #[serde(default)]
    pub usage: Option<SttUsage>,
}

impl SttWord {
    /// Speaker id as a stable key ("speaker_0"), whatever the provider's JSON type.
    pub fn speaker_key(&self) -> Option<String> {
        speaker_key(self.speaker.as_ref())
    }
}

pub fn speaker_key(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::Number(n) => Some(format!("speaker_{}", n)),
        serde_json::Value::String(s) if !s.is_empty() => {
            if s.starts_with("speaker_") {
                Some(s.clone())
            } else {
                Some(format!("speaker_{}", s))
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum SttError {
    /// Non-2xx answer from OpenRouter.
    Http { status: u16, body: String },
    Network(String),
    Parse(String),
}

impl SttError {
    /// Worth retrying the same request (timeouts, rate limits, upstream hiccups).
    pub fn is_retryable(&self) -> bool {
        match self {
            SttError::Network(_) => true,
            SttError::Http { status, .. } => *status == 408 || *status == 429 || *status >= 500,
            SttError::Parse(_) => false,
        }
    }

    /// Key missing/invalid or credits exhausted: retrying cannot help this session.
    pub fn is_auth_or_billing(&self) -> bool {
        matches!(self, SttError::Http { status, .. } if *status == 401 || *status == 402 || *status == 403)
    }

    pub fn is_unsupported_option(&self) -> bool {
        matches!(self, SttError::Http { status: 400, body } if body.contains("does not support"))
    }
}

impl std::fmt::Display for SttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SttError::Http { status, body } => write!(f, "OpenRouter STT HTTP {}: {}", status, truncate(body, 300)),
            SttError::Network(e) => write!(f, "OpenRouter STT network error: {}", e),
            SttError::Parse(e) => write!(f, "OpenRouter STT invalid response: {}", e),
        }
    }
}

impl std::error::Error for SttError {}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

pub fn build_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(8))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub fn build_body(audio: &[u8], format: &str, opts: &SttOptions, with_vocabulary: bool) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": opts.model,
        "input_audio": {
            "data": base64::engine::general_purpose::STANDARD.encode(audio),
            "format": format,
        },
        "temperature": 0,
    });
    if let Some(lang) = opts.language.as_deref().filter(|l| !l.is_empty()) {
        body["language"] = serde_json::json!(lang);
    }
    if opts.verbose || opts.diarize {
        body["response_format"] = serde_json::json!("verbose_json");
        body["timestamp_granularities"] = serde_json::json!(["word", "segment"]);
    }
    if opts.diarize {
        body["diarize"] = serde_json::json!(true);
    }

    let mut provider = serde_json::json!({ "data_collection": "deny" });
    if opts.zdr {
        provider["zdr"] = serde_json::json!(true);
    }
    if with_vocabulary && !opts.vocabulary.is_empty() && opts.model.starts_with("openai/") {
        let hint = format!("Noms propres et termes : {}.", opts.vocabulary.join(", "));
        provider["options"] = serde_json::json!({ "openai": { "prompt": hint } });
    }
    body["provider"] = provider;
    body
}

/// One transcription request. Retries once without vocabulary if the provider rejects it.
pub async fn transcribe(
    client: &reqwest::Client,
    api_key: &str,
    audio: &[u8],
    format: &str,
    opts: &SttOptions,
) -> Result<SttResponse, SttError> {
    match send(client, api_key, &build_body(audio, format, opts, true)).await {
        Err(e) if e.is_unsupported_option() && !opts.vocabulary.is_empty() => {
            send(client, api_key, &build_body(audio, format, opts, false)).await
        }
        other => other,
    }
}

async fn send(client: &reqwest::Client, api_key: &str, body: &serde_json::Value) -> Result<SttResponse, SttError> {
    let response = client
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .header("HTTP-Referer", "https://github.com/loicgoaper/meetily")
        .header("X-Title", "Meetily perso")
        .json(body)
        .send()
        .await
        .map_err(|e| SttError::Network(e.to_string()))?;

    let status = response.status();
    let text = response.text().await.map_err(|e| SttError::Network(e.to_string()))?;
    if !status.is_success() {
        return Err(SttError::Http { status: status.as_u16(), body: text });
    }
    serde_json::from_str::<SttResponse>(&text).map_err(|e| SttError::Parse(format!("{} ({})", e, truncate(&text, 200))))
}

/// Retry policy for live chunks: two extra attempts on transient errors.
pub async fn transcribe_with_retry(
    client: &reqwest::Client,
    api_key: &str,
    audio: &[u8],
    format: &str,
    opts: &SttOptions,
) -> Result<SttResponse, SttError> {
    let delays = [Duration::from_millis(500), Duration::from_secs(2)];
    let mut attempt = 0;
    loop {
        match transcribe(client, api_key, audio, format, opts).await {
            Err(e) if e.is_retryable() && attempt < delays.len() => {
                log::warn!("STT attempt {} failed ({}), retrying", attempt + 1, e);
                tokio::time::sleep(delays[attempt]).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttModelInfo {
    pub id: String,
    pub name: String,
    /// USD per second of audio when the model is priced per second
    pub price_per_second: Option<f64>,
}

/// List the transcription models currently offered by OpenRouter (no key needed).
#[tauri::command]
pub async fn get_openrouter_transcription_models() -> Result<Vec<SttModelInfo>, String> {
    let client = build_client(Duration::from_secs(20));
    let value: serde_json::Value = client
        .get(MODELS_ENDPOINT)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let mut models: Vec<SttModelInfo> = value["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            Some(SttModelInfo {
                id: m["id"].as_str()?.to_string(),
                name: m["name"].as_str().unwrap_or_default().to_string(),
                price_per_second: m["pricing"]["prompt"].as_str().and_then(|p| p.parse::<f64>().ok()),
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_has_language_privacy_and_no_vocabulary_for_non_openai() {
        let opts = SttOptions {
            model: "elevenlabs/scribe-v2".into(),
            language: Some("fr".into()),
            vocabulary: vec!["Acme".into()],
            ..Default::default()
        };
        let body = build_body(b"RIFF", "wav", &opts, true);
        assert_eq!(body["language"], "fr");
        assert_eq!(body["provider"]["data_collection"], "deny");
        assert!(body["provider"].get("options").is_none());
        assert!(body.get("diarize").is_none());
        assert_eq!(body["input_audio"]["data"], "UklGRg==");
    }

    #[test]
    fn openai_models_get_a_prompt_hint() {
        let opts = SttOptions {
            model: "openai/gpt-4o-transcribe".into(),
            vocabulary: vec!["Thibaudier".into(), "Acme".into()],
            ..Default::default()
        };
        let body = build_body(b"x", "wav", &opts, true);
        assert!(body["provider"]["options"]["openai"]["prompt"].as_str().unwrap().contains("Thibaudier, Acme"));
        let without = build_body(b"x", "wav", &opts, false);
        assert!(without["provider"].get("options").is_none());
    }

    #[test]
    fn diarize_requests_verbose_word_timestamps() {
        let opts = SttOptions { model: "elevenlabs/scribe-v2".into(), diarize: true, zdr: true, ..Default::default() };
        let body = build_body(b"x", "ogg", &opts, true);
        assert_eq!(body["diarize"], true);
        assert_eq!(body["response_format"], "verbose_json");
        assert_eq!(body["provider"]["zdr"], true);
    }

    #[test]
    fn parses_elevenlabs_verbose_response() {
        let raw = r#"{"text":"Seule. Il est vrai","usage":{"seconds":300,"cost":0.0183},"language":"fr","duration":300,
            "segments":[{"id":0,"start":0,"end":300,"text":"Seule. Il est vrai"}],
            "words":[{"word":"Seule.","start":0.12,"end":0.4,"speaker":0,"confidence":0.97,"speaker_label":"speaker_0"},
                     {"word":"Il","start":3.8,"end":3.9,"speaker":1,"speaker_label":"speaker_1"}]}"#;
        let r: SttResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(r.words.len(), 2);
        assert_eq!(r.words[0].speaker_key().as_deref(), Some("speaker_0"));
        assert_eq!(r.words[1].speaker_key().as_deref(), Some("speaker_1"));
        assert_eq!(r.usage.unwrap().cost, Some(0.0183));
    }

    #[test]
    fn classifies_errors() {
        assert!(SttError::Http { status: 429, body: String::new() }.is_retryable());
        assert!(SttError::Http { status: 503, body: String::new() }.is_retryable());
        assert!(!SttError::Http { status: 400, body: String::new() }.is_retryable());
        assert!(SttError::Http { status: 402, body: String::new() }.is_auth_or_billing());
        assert!(SttError::Http { status: 400, body: "The selected model does not support keyterms".into() }
            .is_unsupported_option());
    }
}

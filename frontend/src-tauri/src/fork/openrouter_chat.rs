// fork/openrouter_chat.rs
//
// Small OpenRouter chat-completions client for the fork's batch steps (speaker
// naming, screenshot reading). Unlike summary/llm_client.rs it accepts multimodal
// content (text + image_url parts) and reports the call cost.

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

pub const CHAT_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageUrl {
    pub url: String,
}

impl ContentPart {
    pub fn text(text: impl Into<String>) -> Self {
        ContentPart::Text { text: text.into() }
    }

    pub fn jpeg(bytes: &[u8]) -> Self {
        use base64::Engine as _;
        ContentPart::ImageUrl {
            image_url: ImageUrl {
                url: format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Vec<ContentPart>,
}

impl ChatMessage {
    pub fn system(text: impl Into<String>) -> Self {
        Self { role: "system".into(), content: vec![ContentPart::text(text)] }
    }
    pub fn user(content: Vec<ContentPart>) -> Self {
        Self { role: "user".into(), content }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChatOutcome {
    pub text: String,
    pub cost_usd: Option<f64>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    cost: Option<f64>,
}

/// The OpenRouter key saved in Meetily (summary settings). Shared by every cloud step.
pub async fn api_key<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    let state = app.state::<AppState>();
    crate::database::repositories::setting::SettingsRepository::get_api_key(state.db_manager.pool(), "openrouter")
        .await
        .map_err(|e| e.to_string())?
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "Clé OpenRouter manquante (Réglages → Modèle de résumé → OpenRouter)".to_string())
}

pub async fn chat(
    api_key: &str,
    model: &str,
    messages: &[ChatMessage],
    json_output: bool,
    max_tokens: u32,
    zdr: bool,
) -> Result<ChatOutcome, String> {
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": 0.1,
        "max_tokens": max_tokens,
        "usage": { "include": true },
        "provider": { "data_collection": "deny" },
    });
    if zdr {
        body["provider"]["zdr"] = serde_json::json!(true);
    }
    if json_output {
        body["response_format"] = serde_json::json!({ "type": "json_object" });
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(240))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let mut last_error = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(2 * attempt as u64)).await;
        }
        let response = match client
            .post(CHAT_ENDPOINT)
            .bearer_auth(api_key)
            .header("HTTP-Referer", "https://github.com/loicgoaper/meetily")
            .header("X-Title", "Meetily perso")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = e.to_string();
                continue;
            }
        };
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            last_error = format!("OpenRouter HTTP {}: {}", status, text.chars().take(300).collect::<String>());
            if status.as_u16() == 429 || status.is_server_error() {
                continue;
            }
            return Err(last_error);
        }
        let parsed: ChatResponse = serde_json::from_str(&text).map_err(|e| format!("Réponse OpenRouter illisible : {}", e))?;
        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();
        return Ok(ChatOutcome { text: content, cost_usd: parsed.usage.and_then(|u| u.cost) });
    }
    Err(last_error)
}

/// Masked view of the saved key ("sk-or-v1-6b…bc1"); the full key never goes back to the UI.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 12 {
        return "•".repeat(chars.len());
    }
    format!("{}…{}", chars[..10].iter().collect::<String>(), chars[chars.len() - 4..].iter().collect::<String>())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub configured: bool,
    pub masked: Option<String>,
}

#[tauri::command]
pub async fn api_get_openrouter_key_status<R: Runtime>(app: AppHandle<R>) -> Result<KeyStatus, String> {
    Ok(match api_key(&app).await {
        Ok(key) => KeyStatus { configured: true, masked: Some(mask_key(&key)) },
        Err(_) => KeyStatus { configured: false, masked: None },
    })
}

/// Replace the OpenRouter key used by summaries, transcription, speaker naming and vision.
#[tauri::command]
pub async fn api_set_openrouter_key<R: Runtime>(app: AppHandle<R>, key: String) -> Result<KeyStatus, String> {
    let key = key.trim().to_string();
    if !key.starts_with("sk-or-") {
        return Err("Une clé OpenRouter commence par « sk-or- »".to_string());
    }
    let state = app.state::<AppState>();
    crate::database::repositories::setting::SettingsRepository::save_api_key(state.db_manager.pool(), "openrouter", &key)
        .await
        .map_err(|e| e.to_string())?;
    Ok(KeyStatus { configured: true, masked: Some(mask_key(&key)) })
}

/// Extract the first JSON object from a model answer (tolerates ```json fences).
pub fn extract_json(text: &str) -> Option<serde_json::Value> {
    if let Ok(v) = serde_json::from_str(text.trim()) {
        return Some(v);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(&text[start..=end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_parts_serialize_like_openai() {
        let msg = ChatMessage::user(vec![ContentPart::text("hi"), ContentPart::jpeg(b"\xff\xd8")]);
        let v = serde_json::to_value(&msg).unwrap();
        assert_eq!(v["content"][0]["type"], "text");
        assert_eq!(v["content"][1]["type"], "image_url");
        assert!(v["content"][1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn masks_keys() {
        assert_eq!(mask_key("sk-or-v1-0123456789abcdef"), "sk-or-v1-0…cdef");
        assert_eq!(mask_key("short"), "•••••");
    }

    #[test]
    fn extracts_json_from_fenced_answer() {
        let v = extract_json("Voici :\n```json\n{\"a\": 1}\n```").unwrap();
        assert_eq!(v["a"], 1);
        assert!(extract_json("pas de json").is_none());
    }
}

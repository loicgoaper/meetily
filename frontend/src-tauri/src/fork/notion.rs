// fork/notion.rs
//
// One-way backup of meetings to Notion: a "Réunions Meetily" database created under a
// page shared with the user's integration, one page per meeting (properties + summary
// + context + transcript in a toggle heading), updated in place after each change.
// Changes are queued (mark_dirty) and a background worker syncs them 15 s after the
// last edit, so a burst of renames produces a single sync.

use super::meeting_doc::{self, MeetingDoc};
use super::notion_md;
use super::preferences::PreferencesRepository;
use crate::state::AppState;
use chrono::Utc;
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

const API: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";
/// Notion allows ~3 requests per second per integration.
const MIN_INTERVAL: Duration = Duration::from_millis(350);
const DEBOUNCE: Duration = Duration::from_secs(15);
const BATCH: usize = 100;

pub const PREF_TOKEN: &str = "notion.token";
pub const PREF_PARENT: &str = "notion.parent_page_id";
pub const PREF_DATABASE: &str = "notion.database_id";
pub const PREF_DATABASE_URL: &str = "notion.database_url";
pub const PREF_AUTO: &str = "notion.auto_sync";

const PROP_TITLE: &str = "Nom";
const PROP_DATE: &str = "Date";
const PROP_TAGS: &str = "Tags";
const PROP_PEOPLE: &str = "Participants";
const PROP_DURATION: &str = "Durée (min)";
const PROP_COST: &str = "Coût cloud ($)";
const PROP_ID: &str = "Meetily ID";
const DATABASE_TITLE: &str = "Réunions Meetily";

// ---------------------------------------------------------------------------
// Dirty queue
// ---------------------------------------------------------------------------

static DIRTY: Lazy<Mutex<HashMap<String, Instant>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static LAST_REQUEST: Lazy<tokio::sync::Mutex<Instant>> = Lazy::new(|| tokio::sync::Mutex::new(Instant::now() - MIN_INTERVAL));
static SYNC_LOCK: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));

/// Queue a meeting for Notion sync (cheap; called after every meaningful change).
pub fn mark_dirty(meeting_id: &str) {
    if let Ok(mut dirty) = DIRTY.lock() {
        dirty.insert(meeting_id.to_string(), Instant::now());
    }
}

fn take_ready() -> Vec<String> {
    let mut dirty = DIRTY.lock().unwrap_or_else(|e| e.into_inner());
    let ready: Vec<String> = dirty.iter().filter(|(_, at)| at.elapsed() >= DEBOUNCE).map(|(id, _)| id.clone()).collect();
    for id in &ready {
        dirty.remove(id);
    }
    ready
}

/// Background worker, started once at app setup.
pub fn start_worker<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let ready = take_ready();
            if ready.is_empty() {
                continue;
            }
            let pool = app.state::<AppState>().db_manager.pool().clone();
            if PreferencesRepository::get(&pool, PREF_AUTO).await == "false" || !is_configured(&pool).await {
                continue;
            }
            for meeting_id in ready {
                match sync_meeting(&pool, &meeting_id, false).await {
                    Ok(url) => {
                        let _ = app.emit("notion-sync-status", json!({ "meetingId": meeting_id, "ok": true, "url": url }));
                    }
                    Err(e) => {
                        warn!("Notion sync failed for {}: {}", meeting_id, e);
                        let _ = app.emit("notion-sync-status", json!({ "meetingId": meeting_id, "ok": false, "error": e }));
                    }
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// HTTP client
// ---------------------------------------------------------------------------

struct Notion {
    client: reqwest::Client,
    token: String,
}

impl Notion {
    fn new(token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, token }
    }

    async fn request(&self, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Value, String> {
        for attempt in 0..4 {
            {
                let mut last = LAST_REQUEST.lock().await;
                let wait = MIN_INTERVAL.saturating_sub(last.elapsed());
                if !wait.is_zero() {
                    tokio::time::sleep(wait).await;
                }
                *last = Instant::now();
            }
            let mut req = self
                .client
                .request(method.clone(), format!("{}/{}", API, path))
                .bearer_auth(&self.token)
                .header("Notion-Version", NOTION_VERSION);
            if let Some(b) = body {
                req = req.json(b);
            }
            let response = match req.send().await {
                Ok(r) => r,
                Err(e) if attempt < 3 => {
                    warn!("Notion request error ({}), retrying", e);
                    tokio::time::sleep(Duration::from_secs(1 + attempt)).await;
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            let status = response.status();
            if status.as_u16() == 429 || status.is_server_error() {
                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1 + attempt);
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }
            let text = response.text().await.map_err(|e| e.to_string())?;
            if !status.is_success() {
                let message = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["message"].as_str().map(str::to_string))
                    .unwrap_or(text);
                return Err(format!("Notion {} : {}", status.as_u16(), message));
            }
            return serde_json::from_str(&text).map_err(|e| e.to_string());
        }
        Err("Notion ne répond pas (trop de tentatives)".to_string())
    }

    async fn get(&self, path: &str) -> Result<Value, String> {
        self.request(reqwest::Method::GET, path, None).await
    }
    async fn post(&self, path: &str, body: &Value) -> Result<Value, String> {
        self.request(reqwest::Method::POST, path, Some(body)).await
    }
    async fn patch(&self, path: &str, body: &Value) -> Result<Value, String> {
        self.request(reqwest::Method::PATCH, path, Some(body)).await
    }
    async fn delete(&self, path: &str) -> Result<Value, String> {
        self.request(reqwest::Method::DELETE, path, None).await
    }

    /// Append blocks in batches; returns the created blocks.
    async fn append(&self, parent: &str, blocks: Vec<Value>) -> Result<Vec<Value>, String> {
        let mut created = Vec::new();
        for chunk in blocks.chunks(BATCH) {
            let result = self.patch(&format!("blocks/{}/children", parent), &json!({ "children": chunk })).await?;
            created.extend(result["results"].as_array().cloned().unwrap_or_default());
        }
        Ok(created)
    }

    async fn clear_children(&self, parent: &str) -> Result<(), String> {
        loop {
            let page = self.get(&format!("blocks/{}/children?page_size=100", parent)).await?;
            let ids: Vec<String> = page["results"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|b| b["id"].as_str().map(str::to_string))
                .collect();
            if ids.is_empty() {
                return Ok(());
            }
            for id in ids {
                self.delete(&format!("blocks/{}", id)).await?;
            }
        }
    }
}

fn plain_title(value: &Value) -> String {
    value
        .as_array()
        .map(|items| items.iter().filter_map(|t| t["plain_text"].as_str()).collect::<String>())
        .unwrap_or_default()
}

fn text_items(content: &str) -> Value {
    let chars: Vec<char> = content.chars().collect();
    Value::Array(
        chars
            .chunks(1900)
            .map(|c| json!({ "type": "text", "text": { "content": c.iter().collect::<String>() } }))
            .collect(),
    )
}

async fn client(pool: &SqlitePool) -> Result<Notion, String> {
    let token = PreferencesRepository::get(pool, PREF_TOKEN).await;
    if token.trim().is_empty() {
        return Err("Clé Notion manquante (Réglages → Notion)".to_string());
    }
    Ok(Notion::new(token))
}

async fn is_configured(pool: &SqlitePool) -> bool {
    !PreferencesRepository::get(pool, PREF_TOKEN).await.trim().is_empty()
        && !PreferencesRepository::get(pool, PREF_DATABASE).await.trim().is_empty()
}

async fn set_prefs(pool: &SqlitePool, values: &[(&str, &str)]) -> Result<(), String> {
    let map = values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    PreferencesRepository::set_many(pool, &map).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Page content
// ---------------------------------------------------------------------------

fn page_properties(doc: &MeetingDoc) -> Value {
    let mut props = json!({
        PROP_TITLE: { "title": text_items(&doc.title) },
        PROP_TAGS: { "multi_select": doc.tags.iter().map(|t| json!({ "name": t.name.replace(',', " ") })).collect::<Vec<_>>() },
        PROP_PEOPLE: { "rich_text": text_items(&doc.participants_label()) },
        PROP_ID: { "rich_text": text_items(&doc.id) },
        PROP_COST: { "number": (doc.cloud_cost * 1000.0).round() / 1000.0 },
    });
    if let Some(date) = doc.local_date() {
        props[PROP_DATE] = json!({ "date": { "start": date.to_rfc3339() } });
    }
    if let Some(secs) = doc.duration_secs {
        props[PROP_DURATION] = json!({ "number": (secs / 60.0).round() });
    }
    props
}

/// Top-level blocks (summary, context) and the transcript lines that go in the toggle.
fn page_blocks(doc: &MeetingDoc) -> (Vec<Value>, Vec<Value>) {
    let fr = super::i18n::is_french();
    let mut blocks = Vec::new();
    match doc.summary_markdown.as_deref() {
        Some(md) => blocks.extend(notion_md::markdown_to_blocks(md)),
        None => blocks.push(notion_md::paragraph(if fr { "*Pas encore de compte rendu.*" } else { "*No summary yet.*" })),
    }
    if let Some(context) = doc.context_markdown() {
        blocks.push(notion_md::divider());
        // drop the document title line, Notion already shows the page title
        let body: String = context.lines().skip(1).collect::<Vec<_>>().join("\n");
        blocks.push(notion_md::heading(2, if fr { "Contexte" } else { "Context" }, false));
        blocks.extend(notion_md::markdown_to_blocks(&body));
    }
    blocks.push(notion_md::divider());
    blocks.push(notion_md::heading(2, if fr { "Transcription" } else { "Transcript" }, true));

    let mut lines = Vec::new();
    let mut last: Option<&str> = None;
    for line in &doc.lines {
        let mut items = Vec::new();
        if let Some(start) = line.start {
            items.push(json!({ "type": "text", "text": { "content": format!("[{}] ", meeting_doc::mm_ss(start)) }, "annotations": { "color": "gray" } }));
        }
        if let Some(name) = line.speaker.as_deref() {
            if Some(name) != last {
                items.push(json!({ "type": "text", "text": { "content": format!("{} : ", name) }, "annotations": { "bold": true } }));
            }
            last = Some(name);
        }
        for chunk in line.text.chars().collect::<Vec<_>>().chunks(1900) {
            items.push(json!({ "type": "text", "text": { "content": chunk.iter().collect::<String>() } }));
        }
        lines.push(notion_md::plain_paragraph(items));
    }
    (blocks, lines)
}

fn content_hash(props: &Value, blocks: &[Value], lines: &[Value]) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let text = format!("{}{}{}", props, Value::Array(blocks.to_vec()), Value::Array(lines.to_vec()));
    let mut hash = FNV_OFFSET;
    for b in text.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}:{}", hash, text.len())
}

/// Make sure every tag of the meeting exists as a coloured option of the database.
async fn ensure_tag_options(notion: &Notion, database_id: &str, doc: &MeetingDoc) -> Result<(), String> {
    if doc.tags.is_empty() {
        return Ok(());
    }
    let db = notion.get(&format!("databases/{}", database_id)).await?;
    let existing: Vec<Value> = db["properties"][PROP_TAGS]["multi_select"]["options"].as_array().cloned().unwrap_or_default();
    let names: Vec<String> = existing.iter().filter_map(|o| o["name"].as_str().map(|n| n.to_lowercase())).collect();
    let missing: Vec<Value> = doc
        .tags
        .iter()
        .filter(|t| !names.contains(&t.name.replace(',', " ").to_lowercase()))
        .map(|t| json!({ "name": t.name.replace(',', " "), "color": t.color }))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut options: Vec<Value> = existing.iter().map(|o| json!({ "id": o["id"], "name": o["name"] })).collect();
    options.extend(missing);
    notion
        .patch(&format!("databases/{}", database_id), &json!({ "properties": { PROP_TAGS: { "multi_select": { "options": options } } } }))
        .await?;
    Ok(())
}

async fn record_state(pool: &SqlitePool, meeting_id: &str, page_id: Option<&str>, page_url: Option<&str>, hash: Option<&str>, error: Option<&str>) {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT INTO notion_pages (meeting_id, page_id, page_url, content_hash, synced_at, last_error) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(meeting_id) DO UPDATE SET
            page_id = COALESCE(excluded.page_id, notion_pages.page_id),
            page_url = COALESCE(excluded.page_url, notion_pages.page_url),
            content_hash = COALESCE(excluded.content_hash, notion_pages.content_hash),
            synced_at = CASE WHEN excluded.last_error IS NULL THEN excluded.synced_at ELSE notion_pages.synced_at END,
            last_error = excluded.last_error",
    )
    .bind(meeting_id)
    .bind(page_id)
    .bind(page_url)
    .bind(hash)
    .bind(&now)
    .bind(error)
    .execute(pool)
    .await;
    if let Err(e) = result {
        warn!("Could not record Notion sync state: {}", e);
    }
}

/// Create or update the Notion page of a meeting. Returns its URL.
pub async fn sync_meeting(pool: &SqlitePool, meeting_id: &str, force: bool) -> Result<String, String> {
    let _serial = SYNC_LOCK.lock().await;
    let result = sync_meeting_inner(pool, meeting_id, force).await;
    if let Err(ref e) = result {
        record_state(pool, meeting_id, None, None, None, Some(e)).await;
    }
    result
}

async fn sync_meeting_inner(pool: &SqlitePool, meeting_id: &str, force: bool) -> Result<String, String> {
    let notion = client(pool).await?;
    let database_id = PreferencesRepository::get(pool, PREF_DATABASE).await;
    if database_id.trim().is_empty() {
        return Err("Base Notion non configurée (Réglages → Notion)".to_string());
    }
    let doc = meeting_doc::load(pool, meeting_id).await?;
    let props = page_properties(&doc);
    let (blocks, lines) = page_blocks(&doc);
    let hash = content_hash(&props, &blocks, &lines);

    let known: Option<(Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT page_id, page_url, content_hash FROM notion_pages WHERE meeting_id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let (known_page, known_url, known_hash) = known.unwrap_or((None, None, None));
    if !force && known_page.is_some() && known_hash.as_deref() == Some(hash.as_str()) {
        return Ok(known_url.unwrap_or_default());
    }

    ensure_tag_options(&notion, &database_id, &doc).await?;

    // Update in place when the page still exists (and is not archived), else create it
    let mut page: Option<(String, String)> = None;
    if let Some(page_id) = known_page.as_deref() {
        match notion.patch(&format!("pages/{}", page_id), &json!({ "properties": props, "archived": false })).await {
            Ok(updated) => {
                notion.clear_children(page_id).await?;
                page = Some((page_id.to_string(), updated["url"].as_str().unwrap_or_default().to_string()));
            }
            Err(e) => warn!("Notion page {} not updatable ({}), creating a new one", page_id, e),
        }
    }
    let (page_id, page_url) = match page {
        Some(p) => p,
        None => {
            let created = notion
                .post("pages", &json!({ "parent": { "database_id": database_id }, "properties": props }))
                .await?;
            (
                created["id"].as_str().ok_or("Réponse Notion sans identifiant de page")?.to_string(),
                created["url"].as_str().unwrap_or_default().to_string(),
            )
        }
    };

    let created_blocks = notion.append(&page_id, blocks).await?;
    let toggle_id = created_blocks
        .last()
        .and_then(|b| b["id"].as_str())
        .ok_or("Bloc « Transcription » introuvable après l'envoi")?
        .to_string();
    if !lines.is_empty() {
        notion.append(&toggle_id, lines).await?;
    }

    record_state(pool, meeting_id, Some(&page_id), Some(&page_url), Some(&hash), None).await;
    info!("Notion: synced {} -> {}", meeting_id, page_url);
    Ok(page_url)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionStatus {
    pub has_token: bool,
    pub token_masked: Option<String>,
    pub workspace: Option<String>,
    pub integration: Option<String>,
    pub parent_page_id: Option<String>,
    pub database_id: Option<String>,
    pub database_url: Option<String>,
    pub auto_sync: bool,
    pub pending: usize,
    pub error: Option<String>,
}

fn opt(value: String) -> Option<String> {
    if value.trim().is_empty() { None } else { Some(value) }
}

#[tauri::command]
pub async fn api_notion_status(state: tauri::State<'_, AppState>) -> Result<NotionStatus, String> {
    let pool = state.db_manager.pool();
    let token = PreferencesRepository::get(pool, PREF_TOKEN).await;
    let mut status = NotionStatus {
        has_token: !token.trim().is_empty(),
        token_masked: opt(token.clone()).map(|t| super::openrouter_chat::mask_key(&t)),
        workspace: None,
        integration: None,
        parent_page_id: opt(PreferencesRepository::get(pool, PREF_PARENT).await),
        database_id: opt(PreferencesRepository::get(pool, PREF_DATABASE).await),
        database_url: opt(PreferencesRepository::get(pool, PREF_DATABASE_URL).await),
        auto_sync: PreferencesRepository::get(pool, PREF_AUTO).await != "false",
        pending: DIRTY.lock().map(|d| d.len()).unwrap_or(0),
        error: None,
    };
    if status.has_token {
        match Notion::new(token).get("users/me").await {
            Ok(me) => {
                status.integration = me["name"].as_str().map(str::to_string);
                status.workspace = me["bot"]["workspace_name"].as_str().map(str::to_string);
            }
            Err(e) => status.error = Some(e),
        }
    }
    Ok(status)
}

#[tauri::command]
pub async fn api_notion_set_token(state: tauri::State<'_, AppState>, token: String) -> Result<(), String> {
    let token = token.trim().to_string();
    Notion::new(token.clone()).get("users/me").await.map_err(|e| format!("Clé Notion refusée : {}", e))?;
    set_prefs(state.db_manager.pool(), &[(PREF_TOKEN, &token)]).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionPage {
    pub id: String,
    pub title: String,
    pub url: String,
}

/// Pages shared with the integration that can host the meetings database.
#[tauri::command]
pub async fn api_notion_list_pages(state: tauri::State<'_, AppState>) -> Result<Vec<NotionPage>, String> {
    let notion = client(state.db_manager.pool()).await?;
    let result = notion
        .post("search", &json!({ "filter": { "property": "object", "value": "page" }, "page_size": 50 }))
        .await?;
    Ok(result["results"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|p| p["parent"]["type"].as_str() != Some("database_id"))
        .map(|p| {
            let title = p["properties"]
                .as_object()
                .and_then(|props| props.values().find(|v| v["type"] == "title"))
                .map(|v| plain_title(&v["title"]))
                .unwrap_or_default();
            NotionPage {
                id: p["id"].as_str().unwrap_or_default().to_string(),
                title: if title.is_empty() { "(sans titre)".to_string() } else { title },
                url: p["url"].as_str().unwrap_or_default().to_string(),
            }
        })
        .collect())
}

/// Columns of the "Réunions Meetily" database.
fn database_properties(options: Vec<Value>) -> Value {
    json!({
        PROP_TITLE: { "title": {} },
        PROP_DATE: { "date": {} },
        PROP_TAGS: { "multi_select": { "options": options } },
        PROP_PEOPLE: { "rich_text": {} },
        PROP_DURATION: { "number": { "format": "number" } },
        PROP_COST: { "number": { "format": "dollar" } },
        PROP_ID: { "rich_text": {} },
    })
}

/// Columns of `expected` missing from an existing database (`existing` = its "properties").
pub fn missing_properties(expected: &Value, existing: &Value) -> serde_json::Map<String, Value> {
    expected
        .as_object()
        .map(|props| props.iter().filter(|(name, _)| existing.get(name.as_str()).is_none()).map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// A "Réunions Meetily" database already under this page (set up from another PC, or
/// before a reinstall), so it is reused instead of creating a second one.
async fn find_existing_database(notion: &Notion, parent_page_id: &str) -> Result<Option<Value>, String> {
    let children = notion.get(&format!("blocks/{}/children?page_size=100", parent_page_id)).await?;
    for block in children["results"].as_array().into_iter().flatten() {
        if block["type"] == "child_database" && block["child_database"]["title"].as_str() == Some(DATABASE_TITLE) {
            let id = block["id"].as_str().unwrap_or_default();
            let database = notion.get(&format!("databases/{}", id)).await?;
            if database["archived"] != true && database["in_trash"] != true {
                return Ok(Some(database));
            }
        }
    }
    Ok(None)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionDatabase {
    pub url: String,
    /// True when an existing "Réunions Meetily" database was found and reused
    pub reused: bool,
}

/// Set up the "Réunions Meetily" database under the chosen page: reuse the one already
/// there (adding any missing column), or create it.
#[tauri::command]
pub async fn api_notion_create_database(state: tauri::State<'_, AppState>, parent_page_id: String) -> Result<NotionDatabase, String> {
    let pool = state.db_manager.pool();
    let notion = client(pool).await?;
    let tags = super::tags::TagsRepository::list(pool).await.map_err(|e| e.to_string())?;
    let options: Vec<Value> = tags.iter().map(|t| json!({ "name": t.name.replace(',', " "), "color": t.color })).collect();

    let (database, reused) = match find_existing_database(&notion, &parent_page_id).await? {
        Some(existing) => {
            // Tag options are added at sync time; only whole columns are added here
            let missing = missing_properties(&database_properties(Vec::new()), &existing["properties"]);
            if !missing.is_empty() {
                let id = existing["id"].as_str().unwrap_or_default();
                notion.patch(&format!("databases/{}", id), &json!({ "properties": missing })).await?;
            }
            (existing, true)
        }
        None => {
            let body = json!({
                "parent": { "type": "page_id", "page_id": parent_page_id },
                "title": [{ "type": "text", "text": { "content": DATABASE_TITLE } }],
                "properties": database_properties(options),
            });
            (notion.post("databases", &body).await?, false)
        }
    };
    let id = database["id"].as_str().ok_or("Réponse Notion sans identifiant de base")?.to_string();
    let url = database["url"].as_str().unwrap_or_default().to_string();
    let previous = PreferencesRepository::get(pool, PREF_DATABASE).await;
    set_prefs(pool, &[(PREF_PARENT, &parent_page_id), (PREF_DATABASE, &id), (PREF_DATABASE_URL, &url)]).await?;
    // Page links belong to a database: forget them only when the database changes
    if normalize_id(&previous) != normalize_id(&id) {
        sqlx::query("DELETE FROM notion_pages").execute(pool).await.map_err(|e| e.to_string())?;
    }
    Ok(NotionDatabase { url, reused })
}

fn normalize_id(id: &str) -> String {
    id.replace('-', "").to_lowercase()
}

#[tauri::command]
pub async fn api_notion_sync_meeting(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<String, String> {
    sync_meeting(state.db_manager.pool(), &meeting_id, true).await
}

/// Sync every meeting in the background; progress through "notion-sync-progress".
#[tauri::command]
pub async fn api_notion_sync_all<R: Runtime>(app: AppHandle<R>) -> Result<usize, String> {
    let pool = app.state::<AppState>().db_manager.pool().clone();
    if !is_configured(&pool).await {
        return Err("Notion n'est pas configuré (clé et base)".to_string());
    }
    let ids: Vec<String> = sqlx::query_as::<_, (String,)>("SELECT id FROM meetings ORDER BY created_at ASC")
        .fetch_all(&pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(id,)| id)
        .collect();
    let total = ids.len();
    tauri::async_runtime::spawn(async move {
        let mut failed = 0;
        for (index, id) in ids.iter().enumerate() {
            if let Err(e) = sync_meeting(&pool, id, false).await {
                failed += 1;
                warn!("Notion sync failed for {}: {}", id, e);
            }
            let _ = app.emit("notion-sync-progress", json!({ "done": index + 1, "total": total, "failed": failed }));
        }
    });
    Ok(total)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotionMeetingState {
    pub page_url: Option<String>,
    pub synced_at: Option<String>,
    pub last_error: Option<String>,
    pub pending: bool,
}

#[tauri::command]
pub async fn api_notion_meeting_state(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<NotionMeetingState, String> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT page_url, synced_at, last_error FROM notion_pages WHERE meeting_id = ?")
            .bind(&meeting_id)
            .fetch_optional(state.db_manager.pool())
            .await
            .map_err(|e| e.to_string())?;
    let (page_url, synced_at, last_error) = row.unwrap_or((None, None, None));
    Ok(NotionMeetingState {
        page_url,
        synced_at,
        last_error,
        pending: DIRTY.lock().map(|d| d.contains_key(&meeting_id)).unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::meeting_doc::TranscriptLine;

    #[test]
    fn reused_database_gets_only_missing_columns() {
        let existing = json!({ "Nom": {}, "Date": {}, "Tags": {}, "Participants": {}, "Meetily ID": {}, "Notes perso": {} });
        let missing = missing_properties(&database_properties(Vec::new()), &existing);
        let mut names: Vec<&String> = missing.keys().collect();
        names.sort();
        assert_eq!(names, vec!["Coût cloud ($)", "Durée (min)"]);
        assert!(missing_properties(&database_properties(Vec::new()), &database_properties(Vec::new())).is_empty());
    }

    fn doc() -> MeetingDoc {
        MeetingDoc {
            id: "meeting-1".into(),
            title: "Point SIRIS".into(),
            created_at: "2026-10-21T14:00:00+00:00".into(),
            folder: None,
            duration_secs: Some(1830.0),
            tags: vec![super::super::tags::Tag { id: "t".into(), name: "SIRIS".into(), color: "blue".into() }],
            context: Default::default(),
            summary_markdown: Some("**Résumé**\n\nTout va bien.".into()),
            lines: vec![
                TranscriptLine { start: Some(3.0), speaker: Some("Olivier".into()), text: "Bonjour".into() },
                TranscriptLine { start: Some(5.0), speaker: Some("Olivier".into()), text: "On parle de SIRIS".into() },
            ],
            cloud_cost: 0.1234,
        }
    }

    #[test]
    fn properties_match_the_database_schema() {
        let props = page_properties(&doc());
        assert_eq!(props[PROP_TITLE]["title"][0]["text"]["content"], "Point SIRIS");
        assert_eq!(props[PROP_TAGS]["multi_select"][0]["name"], "SIRIS");
        assert_eq!(props[PROP_DURATION]["number"], 31.0);
        assert_eq!(props[PROP_COST]["number"], 0.123);
        assert!(props[PROP_DATE]["date"]["start"].as_str().unwrap().starts_with("2026-10-21"));
    }

    #[test]
    fn transcript_goes_in_a_toggle_and_names_appear_once() {
        let (blocks, lines) = page_blocks(&doc());
        let last = blocks.last().unwrap();
        assert_eq!(last["type"], "heading_2");
        assert_eq!(last["heading_2"]["is_toggleable"], true);
        assert_eq!(lines.len(), 2);
        let first: Vec<&str> = lines[0]["paragraph"]["rich_text"].as_array().unwrap().iter().map(|i| i["text"]["content"].as_str().unwrap()).collect();
        assert_eq!(first, vec!["[00:03] ", "Olivier : ", "Bonjour"]);
        let second = lines[1]["paragraph"]["rich_text"].as_array().unwrap();
        assert_eq!(second.len(), 2, "speaker name not repeated");
    }

    #[test]
    fn hash_changes_with_content() {
        let d = doc();
        let (b, l) = page_blocks(&d);
        let h1 = content_hash(&page_properties(&d), &b, &l);
        let mut d2 = d.clone();
        d2.title = "Autre".into();
        let (b2, l2) = page_blocks(&d2);
        assert_ne!(h1, content_hash(&page_properties(&d2), &b2, &l2));
    }

    #[test]
    fn dirty_queue_waits_for_debounce() {
        mark_dirty("m-debounce");
        assert!(!take_ready().contains(&"m-debounce".to_string()));
    }
}

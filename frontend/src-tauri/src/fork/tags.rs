// fork/tags.rs
//
// Meeting tags (name + Notion colour) to classify and filter meetings by client,
// project, etc. Changes mark the meetings for Notion sync.

use crate::state::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use uuid::Uuid;

/// Colour names accepted by Notion select options.
pub const COLORS: [&str; 10] = ["default", "gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red"];

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
}

fn valid_color(color: &str) -> String {
    if COLORS.contains(&color) { color.to_string() } else { "default".to_string() }
}

pub struct TagsRepository;

impl TagsRepository {
    pub async fn list(pool: &SqlitePool) -> Result<Vec<Tag>, sqlx::Error> {
        sqlx::query_as::<_, Tag>("SELECT id, name, color FROM tags ORDER BY name COLLATE NOCASE")
            .fetch_all(pool)
            .await
    }

    pub async fn for_meeting(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<Tag>, sqlx::Error> {
        sqlx::query_as::<_, Tag>(
            "SELECT t.id, t.name, t.color FROM tags t JOIN meeting_tags mt ON mt.tag_id = t.id
             WHERE mt.meeting_id = ? ORDER BY t.name COLLATE NOCASE",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    pub async fn all_assignments(pool: &SqlitePool) -> Result<HashMap<String, Vec<String>>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String)>("SELECT meeting_id, tag_id FROM meeting_tags")
            .fetch_all(pool)
            .await?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for (meeting, tag) in rows {
            map.entry(meeting).or_default().push(tag);
        }
        Ok(map)
    }

    pub async fn meetings_with_tag(pool: &SqlitePool, tag_id: &str) -> Result<Vec<String>, sqlx::Error> {
        Ok(sqlx::query_as::<_, (String,)>("SELECT meeting_id FROM meeting_tags WHERE tag_id = ?")
            .bind(tag_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|(m,)| m)
            .collect())
    }
}

#[tauri::command]
pub async fn api_list_tags(state: tauri::State<'_, AppState>) -> Result<Vec<Tag>, String> {
    TagsRepository::list(state.db_manager.pool()).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn api_create_tag(state: tauri::State<'_, AppState>, name: String, color: String) -> Result<Tag, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Le nom du tag est obligatoire".to_string());
    }
    let tag = Tag { id: format!("tag-{}", Uuid::new_v4()), name, color: valid_color(&color) };
    sqlx::query("INSERT INTO tags (id, name, color, created_at) VALUES (?, ?, ?, ?)")
        .bind(&tag.id)
        .bind(&tag.name)
        .bind(&tag.color)
        .bind(Utc::now().to_rfc3339())
        .execute(state.db_manager.pool())
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "Un tag porte déjà ce nom".to_string()
            } else {
                e.to_string()
            }
        })?;
    Ok(tag)
}

#[tauri::command]
pub async fn api_update_tag(state: tauri::State<'_, AppState>, id: String, name: String, color: String) -> Result<(), String> {
    let pool = state.db_manager.pool();
    sqlx::query("UPDATE tags SET name = ?, color = ? WHERE id = ?")
        .bind(name.trim())
        .bind(valid_color(&color))
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    for meeting in TagsRepository::meetings_with_tag(pool, &id).await.map_err(|e| e.to_string())? {
        super::notion::mark_dirty(&meeting);
    }
    Ok(())
}

#[tauri::command]
pub async fn api_delete_tag(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let pool = state.db_manager.pool();
    let affected = TagsRepository::meetings_with_tag(pool, &id).await.map_err(|e| e.to_string())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM meeting_tags WHERE tag_id = ?").bind(&id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM tags WHERE id = ?").bind(&id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    for meeting in affected {
        super::notion::mark_dirty(&meeting);
    }
    Ok(())
}

#[tauri::command]
pub async fn api_get_meeting_tags(state: tauri::State<'_, AppState>, meeting_id: String) -> Result<Vec<Tag>, String> {
    TagsRepository::for_meeting(state.db_manager.pool(), &meeting_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn api_set_meeting_tags(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    tag_ids: Vec<String>,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM meeting_tags WHERE meeting_id = ?")
        .bind(&meeting_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for tag_id in &tag_ids {
        sqlx::query("INSERT OR IGNORE INTO meeting_tags (meeting_id, tag_id) VALUES (?, ?)")
            .bind(&meeting_id)
            .bind(tag_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    super::notion::mark_dirty(&meeting_id);
    Ok(())
}

/// meeting id -> tag ids, for the sidebar filter and dots.
#[tauri::command]
pub async fn api_get_all_meeting_tags(state: tauri::State<'_, AppState>) -> Result<HashMap<String, Vec<String>>, String> {
    TagsRepository::all_assignments(state.db_manager.pool()).await.map_err(|e| e.to_string())
}

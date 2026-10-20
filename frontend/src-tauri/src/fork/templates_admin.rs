// fork/templates_admin.rs
//
// Template manager: upstream can only *select* a template; custom ones had to be
// dropped by hand into %APPDATA%\Meetily\templates. These commands let the UI read,
// create, edit, duplicate, import, export and delete them. Edits always go to the
// custom folder; built-in and bundled templates stay untouched (a custom file with
// the same id overrides them, deleting it restores the original).

use crate::summary::templates::{
    get_builtin_template, get_custom_templates_dir, get_template, list_template_ids, load_bundled_template,
    load_custom_template, Template,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    /// "custom" | "bundled" | "builtin"
    pub source: String,
    /// A custom file overrides a built-in/bundled template with the same id
    pub overrides_original: bool,
    pub sections: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateFull {
    pub id: String,
    pub source: String,
    pub overrides_original: bool,
    pub template: Template,
}

fn has_original(id: &str) -> bool {
    get_builtin_template(id).is_some() || load_bundled_template(id).is_some()
}

fn source_of(id: &str) -> &'static str {
    if load_custom_template(id).is_some() {
        "custom"
    } else if load_bundled_template(id).is_some() {
        "bundled"
    } else {
        "builtin"
    }
}

fn custom_dir() -> Result<PathBuf, String> {
    let dir = get_custom_templates_dir().ok_or("Dossier des modèles personnalisés introuvable")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Safe file id from a display name: "Suivi client – Acme" -> "suivi_client_acme".
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let mapped = match c {
            'à' | 'â' | 'ä' | 'á' | 'À' | 'Â' | 'Ä' | 'Á' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'î' | 'ï' | 'í' | 'Î' | 'Ï' | 'Í' => 'i',
            'ô' | 'ö' | 'ó' | 'Ô' | 'Ö' | 'Ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' | 'Ú' => 'u',
            'ç' | 'Ç' => 'c',
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
            _ => '_',
        };
        if mapped == '_' && (out.is_empty() || out.ends_with('_')) {
            continue;
        }
        out.push(mapped);
    }
    let out = out.trim_end_matches('_').to_string();
    if out.is_empty() {
        "modele".to_string()
    } else {
        out.chars().take(60).collect()
    }
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 80 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// An id that is not used by any template yet.
fn unique_id(base: &str) -> String {
    let existing = list_template_ids();
    if !existing.iter().any(|id| id == base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{}_{}", base, n)).find(|id| !existing.contains(id)).unwrap()
}

fn write_template(dir: &Path, id: &str, template: &Template) -> Result<(), String> {
    template.validate()?;
    let json = serde_json::to_string_pretty(template).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".{}.json.tmp", id));
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(format!("{}.json", id))).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn api_list_templates_full() -> Result<Vec<TemplateEntry>, String> {
    Ok(list_template_ids()
        .into_iter()
        .filter_map(|id| {
            let template = get_template(&id).ok()?;
            let source = source_of(&id).to_string();
            Some(TemplateEntry {
                overrides_original: source == "custom" && has_original(&id),
                sections: template.sections.len(),
                name: template.name,
                description: template.description,
                source,
                id,
            })
        })
        .collect())
}

#[tauri::command]
pub async fn api_get_template_full(id: String) -> Result<TemplateFull, String> {
    let template = get_template(&id)?;
    let source = source_of(&id).to_string();
    Ok(TemplateFull { overrides_original: source == "custom" && has_original(&id), source, id, template })
}

/// Save a template to the custom folder. Without `id`, a new id is derived from the name.
/// Returns the id used.
#[tauri::command]
pub async fn api_save_custom_template(id: Option<String>, template: Template) -> Result<String, String> {
    template.validate()?;
    let id = match id.filter(|i| !i.trim().is_empty()) {
        Some(id) if is_valid_id(&id) => id,
        Some(_) => return Err("Identifiant de modèle invalide".to_string()),
        None => unique_id(&slugify(&template.name)),
    };
    write_template(&custom_dir()?, &id, &template)?;
    Ok(id)
}

/// Delete a custom template. For a customised built-in/bundled template this restores
/// the original. Returns true if a template with this id still exists afterwards.
#[tauri::command]
pub async fn api_delete_custom_template(id: String) -> Result<bool, String> {
    if !is_valid_id(&id) {
        return Err("Identifiant de modèle invalide".to_string());
    }
    let path = custom_dir()?.join(format!("{}.json", id));
    if !path.exists() {
        return Err("Seuls les modèles personnalisés peuvent être supprimés".to_string());
    }
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(has_original(&id))
}

#[tauri::command]
pub async fn api_duplicate_template(id: String, new_name: String) -> Result<String, String> {
    let mut template = get_template(&id)?;
    template.name = new_name.trim().to_string();
    if template.name.is_empty() {
        return Err("Le nom du modèle est obligatoire".to_string());
    }
    let new_id = unique_id(&slugify(&template.name));
    write_template(&custom_dir()?, &new_id, &template)?;
    Ok(new_id)
}

/// Ask the user for a template JSON file and import it. Returns the new id, or None if
/// the dialog was cancelled.
#[tauri::command]
pub async fn api_import_template<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    let picked = tokio::task::spawn_blocking(move || app.dialog().file().add_filter("JSON", &["json"]).blocking_pick_file())
        .await
        .map_err(|e| e.to_string())?;
    let Some(path) = picked else { return Ok(None) };
    let path = path.to_string();
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("Lecture impossible : {}", e))?;
    let template = crate::summary::templates::validate_and_parse_template(&raw)?;
    let base = Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(slugify)
        .unwrap_or_else(|| slugify(&template.name));
    let id = unique_id(&base);
    write_template(&custom_dir()?, &id, &template)?;
    Ok(Some(id))
}

/// Ask where to save and export a template as JSON. Returns false if cancelled.
#[tauri::command]
pub async fn api_export_template<R: Runtime>(app: AppHandle<R>, id: String) -> Result<bool, String> {
    let template = get_template(&id)?;
    let json = serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?;
    let file_name = format!("{}.json", id);
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog().file().add_filter("JSON", &["json"]).set_file_name(&file_name).blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = picked else { return Ok(false) };
    std::fs::write(path.to_string(), json).map_err(|e| format!("Écriture impossible : {}", e))?;
    Ok(true)
}

/// Markdown skeleton + instructions the model receives, for the editor preview.
#[tauri::command]
pub async fn api_preview_template(template: Template) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "markdown": template.to_markdown_structure(),
        "instructions": template.to_section_instructions(),
        "valid": template.validate().err(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_handles_french_and_symbols() {
        assert_eq!(slugify("Suivi client – Acme (été)"), "suivi_client_acme_ete");
        assert_eq!(slugify("  ***  "), "modele");
        assert_eq!(slugify("Réunion d'équipe"), "reunion_d_equipe");
    }

    #[test]
    fn rejects_path_like_ids() {
        assert!(!is_valid_id("../evil"));
        assert!(!is_valid_id("a/b"));
        assert!(is_valid_id("fr_reunion_standard"));
    }

    #[test]
    fn french_templates_are_builtin_and_valid() {
        for id in ["fr_reunion_standard", "fr_suivi_projet_client", "fr_reunion_interne", "fr_seance_psychiatrique"] {
            let t = get_template(id).unwrap_or_else(|e| panic!("{}: {}", id, e));
            assert!(t.validate().is_ok());
            assert!(has_original(id));
        }
    }
}

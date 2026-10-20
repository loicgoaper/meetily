/// Embedded default templates using compile-time inclusion
///
/// These templates are bundled into the binary and serve as fallbacks
/// when custom templates are not available.

/// Daily standup template for engineering/product teams
pub const DAILY_STANDUP: &str = include_str!("../../../templates/daily_standup.json");

/// Standard meeting notes template
pub const STANDARD_MEETING: &str = include_str!("../../../templates/standard_meeting.json");

/// Fork (Meetily perso): French templates, embedded so they are always available
pub const FR_REUNION_STANDARD: &str = include_str!("../../../templates/fr_reunion_standard.json");
pub const FR_POINT_QUOTIDIEN: &str = include_str!("../../../templates/fr_point_quotidien.json");
pub const FR_SYNCHRO_PROJET: &str = include_str!("../../../templates/fr_synchro_projet.json");
pub const FR_RETROSPECTIVE: &str = include_str!("../../../templates/fr_retrospective.json");
pub const FR_REUNION_CLIENT_COMMERCIALE: &str = include_str!("../../../templates/fr_reunion_client_commerciale.json");
pub const FR_SEANCE_PSYCHIATRIQUE: &str = include_str!("../../../templates/fr_seance_psychiatrique.json");
pub const FR_SUIVI_PROJET_CLIENT: &str = include_str!("../../../templates/fr_suivi_projet_client.json");
pub const FR_REUNION_INTERNE: &str = include_str!("../../../templates/fr_reunion_interne.json");

/// Registry of all built-in templates
///
/// Maps template identifiers to their embedded JSON content
pub fn get_builtin_templates() -> Vec<(&'static str, &'static str)> {
    vec![
        ("daily_standup", DAILY_STANDUP),
        ("standard_meeting", STANDARD_MEETING),
        ("fr_reunion_standard", FR_REUNION_STANDARD),
        ("fr_point_quotidien", FR_POINT_QUOTIDIEN),
        ("fr_synchro_projet", FR_SYNCHRO_PROJET),
        ("fr_retrospective", FR_RETROSPECTIVE),
        ("fr_reunion_client_commerciale", FR_REUNION_CLIENT_COMMERCIALE),
        ("fr_seance_psychiatrique", FR_SEANCE_PSYCHIATRIQUE),
        ("fr_suivi_projet_client", FR_SUIVI_PROJET_CLIENT),
        ("fr_reunion_interne", FR_REUNION_INTERNE),
    ]
}

/// Get a built-in template by identifier
///
/// # Arguments
/// * `id` - Template identifier (e.g., "daily_standup", "standard_meeting")
///
/// # Returns
/// The template JSON content if found, None otherwise
pub fn get_builtin_template(id: &str) -> Option<&'static str> {
    match id {
        "daily_standup" => Some(DAILY_STANDUP),
        "standard_meeting" => Some(STANDARD_MEETING),
        "fr_reunion_standard" => Some(FR_REUNION_STANDARD),
        "fr_point_quotidien" => Some(FR_POINT_QUOTIDIEN),
        "fr_synchro_projet" => Some(FR_SYNCHRO_PROJET),
        "fr_retrospective" => Some(FR_RETROSPECTIVE),
        "fr_reunion_client_commerciale" => Some(FR_REUNION_CLIENT_COMMERCIALE),
        "fr_seance_psychiatrique" => Some(FR_SEANCE_PSYCHIATRIQUE),
        "fr_suivi_projet_client" => Some(FR_SUIVI_PROJET_CLIENT),
        "fr_reunion_interne" => Some(FR_REUNION_INTERNE),
        _ => None,
    }
}

/// List all built-in template identifiers
pub fn list_builtin_template_ids() -> Vec<&'static str> {
    get_builtin_templates().into_iter().map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_templates_valid_json() {
        for (id, content) in get_builtin_templates() {
            let result = serde_json::from_str::<serde_json::Value>(content);
            assert!(
                result.is_ok(),
                "Built-in template '{}' contains invalid JSON: {:?}",
                id,
                result.err()
            );
        }
    }

    #[test]
    fn test_get_builtin_template() {
        assert!(get_builtin_template("daily_standup").is_some());
        assert!(get_builtin_template("standard_meeting").is_some());
        assert!(get_builtin_template("nonexistent").is_none());
    }
}

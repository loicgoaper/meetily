// fork/i18n.rs
//
// Minimal translation table for the few user-facing strings produced on the Rust
// side (tray menu, notification titles). Everything else is translated in the
// frontend (i18next).

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Runtime};

static UI_FRENCH: AtomicBool = AtomicBool::new(true);

pub fn is_french() -> bool {
    UI_FRENCH.load(Ordering::Relaxed)
}

/// Switch the Rust-side language and refresh the tray menu labels.
pub fn set_ui_language<R: Runtime>(app: &AppHandle<R>, lang: &str) {
    UI_FRENCH.store(!lang.starts_with("en"), Ordering::Relaxed);
    crate::tray::update_tray_menu(app);
}

/// Translate a Rust-side UI string. Unknown keys are returned as-is.
pub fn tr(key: &'static str) -> &'static str {
    if !is_french() {
        return match key {
            "tray.downloading_model" => "⏳ Downloading transcription model...",
            "tray.start" => "Start Recording",
            "tray.starting" => "🔄 Starting Recording...",
            "tray.pause" => "⏸ Pause Recording",
            "tray.stop" => "⏹ Stop Recording",
            "tray.pausing" => "⏸ Pausing...",
            "tray.resume" => "▶ Resume Recording",
            "tray.resuming" => "▶ Resuming...",
            "tray.stopping" => "⏹ Stopping...",
            "tray.open" => "Open Main Window",
            "tray.settings" => "Settings",
            "tray.updates" => "Check for Updates",
            "tray.quit" => "Quit",
            other => other,
        };
    }
    match key {
        "tray.downloading_model" => "⏳ Téléchargement du modèle de transcription...",
        "tray.start" => "Démarrer l'enregistrement",
        "tray.starting" => "🔄 Démarrage de l'enregistrement...",
        "tray.pause" => "⏸ Mettre en pause",
        "tray.stop" => "⏹ Arrêter l'enregistrement",
        "tray.pausing" => "⏸ Mise en pause...",
        "tray.resume" => "▶ Reprendre l'enregistrement",
        "tray.resuming" => "▶ Reprise...",
        "tray.stopping" => "⏹ Arrêt en cours...",
        "tray.open" => "Ouvrir la fenêtre principale",
        "tray.settings" => "Réglages",
        "tray.updates" => "Rechercher des mises à jour",
        "tray.quit" => "Quitter",
        other => other,
    }
}

// Notification texts (system notifications shown by the Rust side)

pub fn notif_recording_started(meeting_name: Option<&str>) -> String {
    match (meeting_name, is_french()) {
        (Some(name), true) => format!("Enregistrement démarré : {}", name),
        (Some(name), false) => format!("Recording started for meeting: {}", name),
        (None, true) => "L'enregistrement a démarré. Pensez à prévenir les participants que la réunion est enregistrée.".to_string(),
        (None, false) => "Recording has started. Please inform others in the meeting that you are recording.".to_string(),
    }
}

pub fn notif_transcription_complete(path: Option<&str>) -> String {
    match (path, is_french()) {
        (Some(p), true) => format!("Transcription terminée et enregistrée dans : {}", p),
        (Some(p), false) => format!("Transcription completed and saved to: {}", p),
        (None, true) => "La transcription est terminée".to_string(),
        (None, false) => "Transcription has been completed".to_string(),
    }
}

pub fn notif_meeting_reminder(title: Option<&str>, minutes: u64) -> String {
    match (title, is_french()) {
        (Some(t), true) => format!("La réunion « {} » commence dans {} minutes", t, minutes),
        (Some(t), false) => format!("Meeting '{}' starts in {} minutes", t, minutes),
        (None, true) => format!("La réunion commence dans {} minutes", minutes),
        (None, false) => format!("Meeting starts in {} minutes", minutes),
    }
}

pub fn notif_text(key: &'static str) -> &'static str {
    let fr = is_french();
    match key {
        "stopped" => if fr { "L'enregistrement est arrêté et sauvegardé" } else { "Recording has been stopped and saved" },
        "paused" => if fr { "L'enregistrement est en pause" } else { "Recording has been paused" },
        "resumed" => if fr { "L'enregistrement a repris" } else { "Recording has been resumed" },
        "error_title" => if fr { "Meetily – Erreur" } else { "Meetily Error" },
        "test" => if fr { "Notification de test : les notifications fonctionnent" } else { "This is a test notification to verify the system is working correctly" },
        other => other,
    }
}

/// Replace accented Latin letters with their ASCII base (folder names stay ASCII so
/// every tool in the audio chain, ffmpeg concat lists included, handles them).
pub fn fold_accents(input: &str) -> String {
    input
        .chars()
        .flat_map(|c| {
            let folded: &str = match c {
                'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => "a",
                'À' | 'Â' | 'Ä' | 'Á' | 'Ã' | 'Å' => "A",
                'é' | 'è' | 'ê' | 'ë' => "e",
                'É' | 'È' | 'Ê' | 'Ë' => "E",
                'î' | 'ï' | 'í' | 'ì' => "i",
                'Î' | 'Ï' | 'Í' | 'Ì' => "I",
                'ô' | 'ö' | 'ó' | 'ò' | 'õ' => "o",
                'Ô' | 'Ö' | 'Ó' | 'Ò' | 'Õ' => "O",
                'ù' | 'û' | 'ü' | 'ú' => "u",
                'Ù' | 'Û' | 'Ü' | 'Ú' => "U",
                'ç' => "c",
                'Ç' => "C",
                'ñ' => "n",
                'Ñ' => "N",
                'œ' => "oe",
                'Œ' => "OE",
                'æ' => "ae",
                'Æ' => "AE",
                '«' | '»' | '“' | '”' => "",
                '’' | '‘' => "'",
                '–' | '—' => "-",
                _ => return vec![c],
            };
            folded.chars().collect::<Vec<char>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_french_accents() {
        assert_eq!(fold_accents("Réunion « Acme » – été"), "Reunion  Acme  - ete");
        assert_eq!(fold_accents("Cœur Ça"), "Coeur Ca");
        assert_eq!(fold_accents("plain_name"), "plain_name");
    }
}

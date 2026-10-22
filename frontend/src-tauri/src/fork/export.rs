// fork/export.rs
//
// Listen to a passage of the recording, and export a meeting (audio, transcript,
// summary, context, screenshots) as a zip, or as a single file when only one part
// is chosen. Upstream could only copy text to the clipboard.

use super::meeting_doc::{self, MeetingDoc};
use crate::state::AppState;
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

/// Extra audio before/after a segment so its first and last words are not cut.
const CLIP_MARGIN_SECS: f64 = 0.5;
const CLIP_MAX_SECS: f64 = 60.0;

fn ffmpeg_command() -> Result<std::process::Command, String> {
    let path = crate::audio::ffmpeg::find_ffmpeg_path().ok_or("ffmpeg introuvable")?;
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(command)
}

/// A passage of the meeting audio as an mp3 data URL (played in the transcript).
#[tauri::command]
pub async fn api_get_audio_clip(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    start: f64,
    end: f64,
) -> Result<String, String> {
    let folder = super::diarization::meeting_folder(state.db_manager.pool(), &meeting_id).await?;
    let audio = crate::audio::retranscription::find_audio_file(&folder).map_err(|e| e.to_string())?;
    let from = (start - CLIP_MARGIN_SECS).max(0.0);
    let len = ((end - start).max(0.5) + 2.0 * CLIP_MARGIN_SECS).min(CLIP_MAX_SECS);
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        use base64::Engine as _;
        let output = ffmpeg_command()?
            .args(["-hide_banner", "-loglevel", "error", "-ss", &format!("{:.3}", from), "-t", &format!("{:.3}", len), "-i"])
            .arg(&audio)
            .args(["-vn", "-ac", "1", "-ar", "22050", "-c:a", "libmp3lame", "-b:a", "64k", "-f", "mp3", "pipe:1"])
            .output()
            .map_err(|e| format!("ffmpeg: {}", e))?;
        if !output.status.success() || output.stdout.is_empty() {
            return Err(format!("Extrait audio impossible : {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(format!("data:audio/mpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(output.stdout)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportParts {
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub transcript: bool,
    #[serde(default)]
    pub summary: bool,
    #[serde(default)]
    pub context: bool,
    #[serde(default)]
    pub screenshots: bool,
}

/// File-name friendly version of a meeting title.
pub fn safe_file_stem(title: &str) -> String {
    let folded = super::i18n::fold_accents(title);
    let stem: String = folded
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
        .collect();
    let stem = stem.split_whitespace().collect::<Vec<_>>().join(" ");
    let stem: String = stem.chars().take(80).collect();
    if stem.trim().is_empty() { "reunion".to_string() } else { stem.trim().to_string() }
}

enum Entry {
    Text(String, String),
    File(String, PathBuf),
}

fn collect_entries(doc: &MeetingDoc, parts: &ExportParts) -> Result<Vec<Entry>, String> {
    let fr = super::i18n::is_french();
    let captures = if fr { "captures" } else { "screenshots" };
    let screens = doc.folder.as_ref().map(|f| f.join(super::screen_capture::SCREENS_DIR));
    let mut entries = Vec::new();
    let mut added: std::collections::HashSet<String> = std::collections::HashSet::new();
    if parts.summary {
        if let Some(md) = doc.summary_document() {
            // Screenshots of the summary travel with it (so a summary alone becomes a zip)
            let (md, files) = super::illustrate::rewrite_screen_links(&md, &format!("{}/", captures), |file| {
                screens.as_ref().is_some_and(|dir| dir.join(file).is_file())
            });
            entries.push(Entry::Text(if fr { "compte-rendu.md" } else { "summary.md" }.into(), md));
            if let Some(dir) = screens.as_ref() {
                for file in files {
                    let name = format!("{}/{}", captures, file);
                    if added.insert(name.clone()) {
                        entries.push(Entry::File(name, dir.join(&file)));
                    }
                }
            }
        }
    }
    if parts.transcript && !doc.lines.is_empty() {
        entries.push(Entry::Text(if fr { "transcription.md" } else { "transcript.md" }.into(), doc.transcript_markdown()));
    }
    if parts.context {
        if let Some(md) = doc.context_markdown() {
            entries.push(Entry::Text(if fr { "contexte.md" } else { "context.md" }.into(), md));
        }
    }
    if let Some(folder) = doc.folder.as_ref() {
        if parts.audio {
            if let Ok(audio) = crate::audio::retranscription::find_audio_file(folder) {
                let ext = audio.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
                entries.push(Entry::File(format!("audio.{}", ext), audio));
            }
        }
        if parts.screenshots {
            let dir = folder.join(super::screen_capture::SCREENS_DIR);
            for frame in super::screen_capture::read_index(&dir).into_iter().filter(|f| !f.excluded && !f.purged) {
                let path = dir.join(&frame.file);
                let name = format!("{}/{}", captures, frame.file);
                if path.is_file() && added.insert(name.clone()) {
                    entries.push(Entry::File(name, path));
                }
            }
        }
    }
    Ok(entries)
}

fn write_zip(target: &Path, entries: Vec<Entry>) -> Result<(), String> {
    let file = std::fs::File::create(target).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
    let text_options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    // Audio and JPEG are already compressed
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for entry in entries {
        match entry {
            Entry::Text(name, content) => {
                zip.start_file(name, text_options).map_err(|e| e.to_string())?;
                zip.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
            }
            Entry::File(name, path) => {
                zip.start_file(name, stored).map_err(|e| e.to_string())?;
                let mut source = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                std::io::copy(&mut source, &mut zip).map_err(|e| e.to_string())?;
            }
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Ask where to save, then export. Returns the saved path, or None if cancelled.
#[tauri::command]
pub async fn api_export_meeting<R: Runtime>(app: AppHandle<R>, meeting_id: String, parts: ExportParts) -> Result<Option<String>, String> {
    let pool = app.state::<AppState>().db_manager.pool().clone();
    let doc = meeting_doc::load(&pool, &meeting_id).await?;
    let entries = collect_entries(&doc, &parts)?;
    if entries.is_empty() {
        return Err(if super::i18n::is_french() { "Rien à exporter pour cette réunion" } else { "Nothing to export for this meeting" }.to_string());
    }
    let stem = safe_file_stem(&doc.title);
    let (file_name, filter_name, extension) = if entries.len() == 1 {
        match &entries[0] {
            Entry::Text(name, _) => (format!("{} - {}", stem, name), "Markdown", "md".to_string()),
            Entry::File(name, _) => {
                let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("bin").to_string();
                (format!("{}.{}", stem, ext), "Audio", ext)
            }
        }
    } else {
        (format!("{}.zip", stem), "Zip", "zip".to_string())
    };

    let dialog_app = app.clone();
    let picked = tokio::task::spawn_blocking(move || {
        dialog_app.dialog().file().add_filter(filter_name, &[extension.as_str()]).set_file_name(&file_name).blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(target) = picked else { return Ok(None) };
    let target = PathBuf::from(target.to_string());
    let saved = target.display().to_string();

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        if entries.len() == 1 {
            match entries.into_iter().next().unwrap() {
                Entry::Text(_, content) => std::fs::write(&target, content).map_err(|e| e.to_string()),
                Entry::File(_, path) => std::fs::copy(&path, &target).map(|_| ()).map_err(|e| e.to_string()),
            }
        } else {
            write_zip(&target, entries)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(Some(saved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stems_are_safe() {
        assert_eq!(safe_file_stem("Réunion « Acme » : bilan/T3"), "Reunion Acme _ bilan_T3");
        assert_eq!(safe_file_stem("   "), "reunion");
        assert_eq!(safe_file_stem("Point SIRIS – Olivier"), "Point SIRIS - Olivier");
    }

    #[test]
    fn summary_screenshots_are_exported_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let screens = dir.path().join(super::super::screen_capture::SCREENS_DIR);
        std::fs::create_dir_all(&screens).unwrap();
        std::fs::write(screens.join("0029_22-56.webp"), b"img").unwrap();
        super::super::screen_capture::write_index(
            &screens,
            &[super::super::screen_capture::ScreenFrame { file: "0029_22-56.webp".into(), t: 1376.0, excluded: false, change: 0.5, monitor: None, purged: false }],
        )
        .unwrap();
        let doc = MeetingDoc {
            id: "m1".into(),
            title: "Salon".into(),
            created_at: "2026-10-21T19:56:29+00:00".into(),
            folder: Some(dir.path().to_path_buf()),
            duration_secs: None,
            tags: Vec::new(),
            context: Default::default(),
            summary_markdown: Some("Stand réservé.\n\n![Formulaire du stand](meetily-screen://0029_22-56.webp)\n".into()),
            lines: Vec::new(),
            cloud_cost: 0.0,
        };
        // Summary alone, and summary + all screenshots: the image is added once
        for screenshots in [false, true] {
            let parts = ExportParts { summary: true, screenshots, ..Default::default() };
            let entries = collect_entries(&doc, &parts).unwrap();
            assert_eq!(entries.len(), 2);
            match &entries[0] {
                Entry::Text(_, md) => assert!(md.contains("![Formulaire du stand](") && md.contains("/0029_22-56.webp)")),
                Entry::File(..) => panic!("summary first"),
            }
            assert!(matches!(&entries[1], Entry::File(name, _) if name.ends_with("/0029_22-56.webp")));
        }
    }

    #[test]
    fn zip_contains_text_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("audio.mp4");
        std::fs::write(&audio, b"fake audio").unwrap();
        let target = dir.path().join("out.zip");
        write_zip(
            &target,
            vec![Entry::Text("compte-rendu.md".into(), "# Titre".into()), Entry::File("audio.mp4".into(), audio)],
        )
        .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&target).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        let mut content = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("compte-rendu.md").unwrap(), &mut content).unwrap();
        assert_eq!(content, "# Titre");
    }
}

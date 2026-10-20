// fork/screen_capture.rs
//
// "Smart screenshots": while a meeting is recorded, capture the chosen screen every
// few seconds and keep a frame only when the content really changed (new slide, new
// document) and has settled. Frames go to <meeting folder>/screens/ with an index
// aligned on the recording's active time (pauses excluded), so the vision step can
// cite them with the same [mm:ss] as the transcript.

use crate::audio::recording_state::RecordingState;
use crate::state::AppState;
use log::{info, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const SCREENS_DIR: &str = "screens";
pub const INDEX_FILE: &str = "index.json";
const MAX_FRAMES: u32 = 300;
const MAX_WIDTH: u32 = 1600;
const MIN_GAP_SECS: f64 = 10.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenFrame {
    pub file: String,
    /// Seconds of active recording (same time base as the transcript)
    pub t: f64,
    #[serde(default)]
    pub excluded: bool,
    #[serde(default)]
    pub change: f32,
}

// ---------------------------------------------------------------------------
// Change detection (pure, tested)
// ---------------------------------------------------------------------------

pub mod change_detect {
    /// 32x18 grayscale thumbnail of a frame.
    pub const GRID_W: u32 = 32;
    pub const GRID_H: u32 = 18;
    /// A cell counts as changed above this luminance difference (0-255).
    const CELL_DELTA: i16 = 12;
    /// Share of changed cells for a new screen.
    pub const NEW_SCREEN: f32 = 0.15;
    /// Below this share of changed cells, the screen is considered settled.
    pub const SETTLED: f32 = 0.08;

    pub type Signature = Vec<u8>;

    pub fn signature_from_luma(pixels: &[u8]) -> Signature {
        pixels.to_vec()
    }

    pub fn changed_share(a: &Signature, b: &Signature) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 1.0;
        }
        let changed = a.iter().zip(b).filter(|(x, y)| (**x as i16 - **y as i16).abs() > CELL_DELTA).count();
        changed as f32 / a.len() as f32
    }

    /// Decides what to do with each new capture.
    #[derive(Default)]
    pub struct Detector {
        last_kept: Option<Signature>,
        last_kept_t: Option<f64>,
        candidate: Option<(Signature, f64, f32, u8)>,
    }

    #[derive(Debug, PartialEq)]
    pub enum Decision {
        /// Nothing new.
        Skip,
        /// Changed, wait for the next capture to see if it settles.
        Wait,
        /// Save this capture (with its change score).
        Keep(f32),
    }

    impl Detector {
        pub fn observe(&mut self, sig: Signature, t: f64, min_gap: f64) -> Decision {
            if let Some((cand, cand_t, change, ticks)) = self.candidate.take() {
                let settled = changed_share(&cand, &sig) < SETTLED;
                if settled || ticks >= 2 {
                    self.last_kept = Some(sig);
                    self.last_kept_t = Some(t);
                    let _ = cand_t;
                    return Decision::Keep(change);
                }
                // still moving: follow the newest image
                self.candidate = Some((sig, t, change, ticks + 1));
                return Decision::Wait;
            }
            let change = match &self.last_kept {
                None => 1.0,
                Some(last) => changed_share(last, &sig),
            };
            let gap_ok = self.last_kept_t.map(|lt| t - lt >= min_gap).unwrap_or(true);
            if change >= NEW_SCREEN && gap_ok {
                self.candidate = Some((sig, t, change, 0));
                Decision::Wait
            } else {
                Decision::Skip
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

struct Session {
    stop: Arc<AtomicBool>,
    frames: Arc<AtomicU32>,
    handle: tokio::task::JoinHandle<()>,
}

struct RecordingContext {
    state: Arc<RecordingState>,
    folder: PathBuf,
}

static SESSION: Lazy<Mutex<Option<Session>>> = Lazy::new(|| Mutex::new(None));
static RECORDING: Lazy<Mutex<Option<RecordingContext>>> = Lazy::new(|| Mutex::new(None));

/// Hook from audio/recording_commands.rs when a recording starts.
pub fn on_recording_started<R: Runtime>(app: &AppHandle<R>, state: Arc<RecordingState>, folder: Option<PathBuf>) {
    let Some(folder) = folder else {
        info!("Screen capture unavailable: recording is not saved to a folder");
        return;
    };
    *RECORDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(RecordingContext { state, folder });

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let requested = match super::meeting_context::live_context().and_then(|c| c.capture_screen) {
            Some(flag) => flag,
            None => super::preferences::get_pref_bool(&app, super::preferences::SCREEN_DEFAULT_ENABLED).await,
        };
        if requested {
            if let Err(e) = start(&app).await {
                warn!("Screen capture could not start: {}", e);
                let _ = app.emit("screen-capture-status", serde_json::json!({ "active": false, "error": e }));
            }
        }
    });
}

/// Hook from audio/recording_commands.rs when a recording stops.
pub async fn on_recording_stopped<R: Runtime>(app: &AppHandle<R>) {
    stop(app).await;
    *RECORDING.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

async fn start<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if SESSION.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        return Ok(());
    }
    let (state, folder) = {
        let guard = RECORDING.lock().unwrap_or_else(|e| e.into_inner());
        let ctx = guard.as_ref().ok_or("Aucun enregistrement en cours")?;
        (ctx.state.clone(), ctx.folder.clone())
    };
    let interval = super::preferences::get_pref(app, super::preferences::SCREEN_INTERVAL_SECONDS)
        .await
        .parse::<f64>()
        .unwrap_or(5.0)
        .clamp(2.0, 60.0);
    let monitor = super::preferences::get_pref(app, "screen.monitor").await;
    let dir = folder.join(SCREENS_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let existing = read_index(&dir);

    // Keep Meetily itself out of the captures (and out of the user's screen shares)
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_content_protected(true);
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    let frames = Arc::new(AtomicU32::new(existing.len() as u32));
    let handle = tokio::spawn(capture_loop(
        app.clone(),
        state,
        dir,
        existing,
        monitor,
        interval,
        stop_flag.clone(),
        frames.clone(),
    ));
    *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = Some(Session { stop: stop_flag, frames, handle });
    info!("🖥️ Screen capture started (every {:.0}s)", interval);
    let _ = app.emit("screen-capture-status", serde_json::json!({ "active": true }));
    Ok(())
}

async fn stop<R: Runtime>(app: &AppHandle<R>) {
    let session = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(session) = session {
        session.stop.store(true, Ordering::SeqCst);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), session.handle).await;
        info!("🖥️ Screen capture stopped ({} frames)", session.frames.load(Ordering::SeqCst));
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_content_protected(false);
        }
        let _ = app.emit("screen-capture-status", serde_json::json!({ "active": false }));
    }
}

#[allow(clippy::too_many_arguments)]
async fn capture_loop<R: Runtime>(
    app: AppHandle<R>,
    state: Arc<RecordingState>,
    dir: PathBuf,
    mut index: Vec<ScreenFrame>,
    monitor: String,
    interval: f64,
    stop_flag: Arc<AtomicBool>,
    frames: Arc<AtomicU32>,
) {
    let mut detector = change_detect::Detector::default();
    // Active time at which the current change was first seen
    let mut change_started: Option<f64> = None;
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs_f64(interval));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    while !stop_flag.load(Ordering::SeqCst) {
        ticker.tick().await;
        if stop_flag.load(Ordering::SeqCst) || frames.load(Ordering::SeqCst) >= MAX_FRAMES {
            if frames.load(Ordering::SeqCst) >= MAX_FRAMES {
                break;
            }
            continue;
        }
        if state.is_paused() {
            continue;
        }
        let t = state.get_active_recording_duration().unwrap_or(0.0);
        let monitor_choice = monitor.clone();
        let captured = tokio::task::spawn_blocking(move || capture(&monitor_choice)).await;
        let image = match captured {
            Ok(Ok(img)) => img,
            Ok(Err(e)) => {
                warn!("Screen capture failed: {}", e);
                continue;
            }
            Err(e) => {
                warn!("Screen capture task failed: {}", e);
                continue;
            }
        };
        let sig = signature(&image);
        match detector.observe(sig, t, MIN_GAP_SECS) {
            change_detect::Decision::Skip => change_started = None,
            change_detect::Decision::Wait => {
                change_started.get_or_insert(t);
            }
            change_detect::Decision::Keep(change) => {
                // Keep the settled (latest) image, timestamped when the change appeared
                let first_seen = change_started.take().unwrap_or(t);
                let n = index.len() + 1;
                let file = format!("{:04}_{}.jpg", n, mm_ss(first_seen).replace(':', "-"));
                let path = dir.join(&file);
                let save = tokio::task::spawn_blocking(move || save_jpeg(&image, &path)).await;
                match save {
                    Ok(Ok(())) => {
                        index.push(ScreenFrame { file: file.clone(), t: first_seen, excluded: false, change });
                        if let Err(e) = write_index(&dir, &index) {
                            warn!("Could not write screens index: {}", e);
                        }
                        let count = frames.fetch_add(1, Ordering::SeqCst) + 1;
                        let _ = app.emit("screen-capture-frame", serde_json::json!({ "file": file, "t": first_seen, "count": count }));
                    }
                    Ok(Err(e)) => warn!("Could not save screenshot: {}", e),
                    Err(e) => warn!("Screenshot save task failed: {}", e),
                }
            }
        }
    }
}

pub fn mm_ss(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn signature(image: &image::RgbaImage) -> change_detect::Signature {
    let small = image::imageops::resize(
        image,
        change_detect::GRID_W,
        change_detect::GRID_H,
        image::imageops::FilterType::Triangle,
    );
    let luma: Vec<u8> = small
        .pixels()
        .map(|p| ((p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000) as u8)
        .collect();
    change_detect::signature_from_luma(&luma)
}

#[cfg(windows)]
fn capture(monitor_choice: &str) -> Result<image::RgbaImage, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let wanted: Option<u32> = monitor_choice.strip_prefix("monitor:").and_then(|id| id.parse().ok());
    let monitor = monitors
        .iter()
        .find(|m| wanted.is_some() && m.id().ok() == wanted)
        .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
        .or_else(|| monitors.first())
        .ok_or("Aucun écran détecté")?;
    monitor.capture_image().map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn capture(_monitor_choice: &str) -> Result<image::RgbaImage, String> {
    Err("La capture d'écran n'est disponible que sous Windows dans cette version".to_string())
}

fn save_jpeg(image: &image::RgbaImage, path: &Path) -> Result<(), String> {
    let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
    let rgb = if rgb.width() > MAX_WIDTH {
        let height = (rgb.height() as f64 * MAX_WIDTH as f64 / rgb.width() as f64) as u32;
        image::imageops::resize(&rgb, MAX_WIDTH, height, image::imageops::FilterType::Triangle)
    } else {
        rgb
    };
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut writer = std::io::BufWriter::new(file);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 80)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())
}

pub fn read_index(dir: &Path) -> Vec<ScreenFrame> {
    std::fs::read_to_string(dir.join(INDEX_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn write_index(dir: &Path, index: &[ScreenFrame]) -> std::io::Result<()> {
    let tmp = dir.join(".index.json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(index)?)?;
    std::fs::rename(tmp, dir.join(INDEX_FILE))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub primary: bool,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub async fn api_list_monitors() -> Result<Vec<MonitorInfo>, String> {
    #[cfg(windows)]
    {
        // xcap monitors hold raw handles (not Send): read them on the blocking thread
        tokio::task::spawn_blocking(|| -> Result<Vec<MonitorInfo>, String> {
            let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
            Ok(monitors
                .iter()
                .map(|m| MonitorInfo {
                    id: format!("monitor:{}", m.id().unwrap_or_default()),
                    name: m.friendly_name().or_else(|_| m.name()).unwrap_or_default(),
                    primary: m.is_primary().unwrap_or(false),
                    width: m.width().unwrap_or_default(),
                    height: m.height().unwrap_or_default(),
                })
                .collect())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
pub async fn api_start_screen_capture<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    start(&app).await
}

#[tauri::command]
pub async fn api_stop_screen_capture<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    stop(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn api_screen_capture_status() -> serde_json::Value {
    let guard = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(s) => serde_json::json!({ "active": true, "frames": s.frames.load(Ordering::SeqCst) }),
        None => serde_json::json!({ "active": false, "frames": 0 }),
    }
}

async fn screens_dir(state: &tauri::State<'_, AppState>, meeting_id: &str) -> Result<PathBuf, String> {
    Ok(super::diarization::meeting_folder(state.db_manager.pool(), meeting_id).await?.join(SCREENS_DIR))
}

#[tauri::command]
pub async fn api_list_meeting_screenshots(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<ScreenFrame>, String> {
    match screens_dir(&state, &meeting_id).await {
        Ok(dir) => Ok(read_index(&dir)),
        Err(_) => Ok(Vec::new()),
    }
}

/// A screenshot as a data URL (thumbnail or full size), so the asset protocol scope
/// does not have to include the recordings folder.
#[tauri::command]
pub async fn api_get_screenshot(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    file: String,
    thumbnail: bool,
) -> Result<String, String> {
    if file.contains('/') || file.contains('\\') || file.contains("..") {
        return Err("Nom de fichier invalide".to_string());
    }
    let path = screens_dir(&state, &meeting_id).await?.join(&file);
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        use base64::Engine as _;
        let bytes = if thumbnail {
            let img = image::open(&path).map_err(|e| e.to_string())?;
            let small = img.thumbnail(360, 240).to_rgb8();
            let mut out = std::io::Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 70)
                .encode_image(&small)
                .map_err(|e| e.to_string())?;
            out.into_inner()
        } else {
            std::fs::read(&path).map_err(|e| e.to_string())?
        };
        Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Exclude (or re-include) a screenshot from the vision step, e.g. a screen showing
/// something private.
#[tauri::command]
pub async fn api_set_screenshot_excluded(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
    file: String,
    excluded: bool,
) -> Result<(), String> {
    let dir = screens_dir(&state, &meeting_id).await?;
    let mut index = read_index(&dir);
    let frame = index.iter_mut().find(|f| f.file == file).ok_or("Capture introuvable")?;
    frame.excluded = excluded;
    write_index(&dir, &index).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::change_detect::{Decision, Detector};

    fn sig(value: u8) -> Vec<u8> {
        vec![value; 32 * 18]
    }

    fn sig_partly(base: u8, other: u8, share: f32) -> Vec<u8> {
        let n = 32 * 18;
        let changed = (n as f32 * share) as usize;
        (0..n).map(|i| if i < changed { other } else { base }).collect()
    }

    #[test]
    fn first_screen_is_kept_once_settled() {
        let mut d = Detector::default();
        assert_eq!(d.observe(sig(100), 0.0, 10.0), Decision::Wait);
        assert!(matches!(d.observe(sig(100), 5.0, 10.0), Decision::Keep(_)));
        assert_eq!(d.observe(sig(100), 10.0, 10.0), Decision::Skip);
    }

    #[test]
    fn small_changes_like_a_webcam_tile_are_ignored() {
        let mut d = Detector::default();
        d.observe(sig(100), 0.0, 10.0);
        d.observe(sig(100), 5.0, 10.0);
        assert_eq!(d.observe(sig_partly(100, 200, 0.10), 20.0, 10.0), Decision::Skip);
    }

    #[test]
    fn new_slide_is_kept_after_min_gap_and_settling() {
        let mut d = Detector::default();
        d.observe(sig(100), 0.0, 10.0);
        d.observe(sig(100), 5.0, 10.0);
        // too soon after the previous kept frame
        assert_eq!(d.observe(sig(30), 8.0, 10.0), Decision::Skip);
        assert_eq!(d.observe(sig(30), 20.0, 10.0), Decision::Wait);
        assert!(matches!(d.observe(sig(30), 25.0, 10.0), Decision::Keep(c) if c > 0.9));
    }

    #[test]
    fn never_settling_screen_is_kept_after_a_few_ticks() {
        let mut d = Detector::default();
        d.observe(sig(10), 0.0, 10.0);
        d.observe(sig(10), 5.0, 10.0);
        assert_eq!(d.observe(sig(200), 20.0, 10.0), Decision::Wait);
        assert_eq!(d.observe(sig(120), 25.0, 10.0), Decision::Wait);
        assert_eq!(d.observe(sig(60), 30.0, 10.0), Decision::Wait);
        assert!(matches!(d.observe(sig(0), 35.0, 10.0), Decision::Keep(_)));
    }

    #[test]
    fn formats_minutes_seconds() {
        assert_eq!(super::mm_ss(125.7), "02:05");
        assert_eq!(super::mm_ss(-1.0), "00:00");
    }
}

// fork/audio_route.rs
//
// Which output carries the meeting. Windows has two default outputs: "multimedia"
// (what upstream Meetily records when no device is chosen) and "communications" (where
// Teams, Zoom... play calls). When they differ, a call heard in a headset was recorded
// from the laptop speakers, i.e. silence. At recording start we pick the output where
// sound is actually playing, else the communications one; during the recording, an
// alert is sent when the recorded output stays silent.

use log::{info, warn};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};

/// Peak level (0..1) above which an output is considered playing.
const PLAYING_PEAK: f32 = 0.001;
/// Silence of the recorded output before alerting.
const SILENT_ALERT_SECS: f64 = 30.0;
/// Below this mean square a system window is digital silence (nothing played at all).
const DIGITAL_SILENCE: f32 = 1e-10;
/// Mic windows above this count as someone speaking (~ -50 dBFS).
const MIC_ACTIVE: f32 = 1e-5;

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub id: String,
    pub name: String,
    pub peak: f32,
}

/// Output to record instead of the multimedia default (None: keep the default).
pub fn pick(multimedia: &Output, communications: &Output) -> Option<String> {
    if multimedia.id == communications.id {
        return None;
    }
    let media_playing = multimedia.peak > PLAYING_PEAK;
    let comms_playing = communications.peak > PLAYING_PEAK;
    if media_playing && !comms_playing {
        None
    } else {
        // Calls go to the communications output; when nothing plays yet, that is
        // where the meeting will be heard
        Some(communications.name.clone())
    }
}

/// Live level of an audio endpoint, for the start dialog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointLevel {
    /// Friendly name, as cpal names the device
    pub name: String,
    pub peak: f32,
    /// Default multimedia output / default microphone
    pub is_default: bool,
    /// Default communications output (where calls play)
    pub is_communications: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AudioLevels {
    pub outputs: Vec<EndpointLevel>,
    pub inputs: Vec<EndpointLevel>,
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{AudioLevels, EndpointLevel, Output};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{
        eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};

    unsafe fn describe(device: &IMMDevice) -> windows::core::Result<(String, String, Option<IAudioMeterInformation>)> {
        let id = device.GetId()?.to_string().unwrap_or_default();
        let store = device.OpenPropertyStore(STGM_READ)?;
        let name = store.GetValue(&PKEY_Device_FriendlyName)?.to_string();
        let meter = device.Activate::<IAudioMeterInformation>(CLSCTX_ALL, None).ok();
        Ok((id, name, meter))
    }

    /// Every active output and microphone with its current peak level (one reading).
    pub fn levels() -> Option<AudioLevels> {
        std::thread::spawn(|| unsafe {
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result = (|| -> windows::core::Result<AudioLevels> {
                let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let default_id = |flow: EDataFlow, role: ERole| {
                    enumerator
                        .GetDefaultAudioEndpoint(flow, role)
                        .and_then(|d| d.GetId())
                        .map(|id| id.to_string().unwrap_or_default())
                        .unwrap_or_default()
                };
                let collect = |flow: EDataFlow, default: String, communications: String| -> windows::core::Result<Vec<EndpointLevel>> {
                    let devices = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
                    let mut out = Vec::new();
                    for i in 0..devices.GetCount()? {
                        let Ok((id, name, meter)) = devices.Item(i).and_then(|d| describe(&d)) else { continue };
                        let peak = meter.and_then(|m| m.GetPeakValue().ok()).unwrap_or(0.0);
                        out.push(EndpointLevel { name, peak, is_default: id == default, is_communications: id == communications });
                    }
                    Ok(out)
                };
                Ok(AudioLevels {
                    outputs: collect(eRender, default_id(eRender, eMultimedia), default_id(eRender, eCommunications))?,
                    inputs: collect(eCapture, default_id(eCapture, eConsole), default_id(eCapture, eCommunications))?,
                })
            })();
            if initialized {
                CoUninitialize();
            }
            result.ok()
        })
        .join()
        .ok()
        .flatten()
    }

    /// Default multimedia and communications outputs with their peak level, sampled
    /// a few times (runs on its own thread: COM in MTA mode).
    pub fn default_outputs() -> Option<(Output, Output)> {
        std::thread::spawn(|| unsafe {
            let initialized = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result = (|| -> windows::core::Result<(Output, Output)> {
                let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let endpoint = |role: ERole| enumerator.GetDefaultAudioEndpoint(eRender, role).and_then(|d| describe(&d));
                let (media_id, media_name, media_meter) = endpoint(eMultimedia)?;
                let (comms_id, comms_name, comms_meter) = endpoint(eCommunications)?;
                let (mut media_peak, mut comms_peak) = (0f32, 0f32);
                for _ in 0..12 {
                    media_peak = media_peak.max(media_meter.as_ref().and_then(|m| m.GetPeakValue().ok()).unwrap_or(0.0));
                    comms_peak = comms_peak.max(comms_meter.as_ref().and_then(|m| m.GetPeakValue().ok()).unwrap_or(0.0));
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Ok((
                    Output { id: media_id, name: media_name, peak: media_peak },
                    Output { id: comms_id, name: comms_name, peak: comms_peak },
                ))
            })();
            if initialized {
                CoUninitialize();
            }
            result.ok()
        })
        .join()
        .ok()
        .flatten()
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    pub fn default_outputs() -> Option<(super::Output, super::Output)> {
        None
    }
    pub fn levels() -> Option<super::AudioLevels> {
        None
    }
}

/// Is an output with exactly this name plugged in? (true when it cannot be checked)
pub fn output_exists(name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        if let Ok(host) = cpal::host_from_id(cpal::HostId::Wasapi) {
            if let Ok(devices) = host.output_devices() {
                return devices.filter_map(|d| d.name().ok()).any(|n| n == name);
            }
        }
    }
    let _ = name;
    true
}

/// Live levels of every output and microphone (start dialog gauges).
#[tauri::command]
pub async fn api_audio_levels() -> Result<AudioLevels, String> {
    tokio::task::spawn_blocking(platform::levels).await.map_err(|e| e.to_string()).map(Option::unwrap_or_default)
}

/// Name of the output the meeting is heard on, when it is not the multimedia default.
pub fn meeting_output() -> Option<String> {
    let (multimedia, communications) = platform::default_outputs()?;
    let chosen = pick(&multimedia, &communications);
    if let Some(name) = chosen.as_ref() {
        info!(
            "🔊 Default outputs differ (multimedia '{}' peak {:.3}, communications '{}' peak {:.3}): recording '{}'",
            multimedia.name, multimedia.peak, communications.name, communications.peak, name
        );
    }
    chosen
}

static SESSION: AtomicU64 = AtomicU64::new(0);

/// True when the last `window_count` system windows are digital silence while the mic
/// picked up speech, over at least SILENT_ALERT_SECS.
pub fn system_silent(windows: &[(f32, f32)], window_secs: f64) -> bool {
    if window_secs <= 0.0 {
        return false;
    }
    let needed = (SILENT_ALERT_SECS / window_secs).ceil() as usize;
    if windows.len() < needed {
        return false;
    }
    let recent = &windows[windows.len() - needed..];
    recent.iter().all(|(_, sys)| *sys < DIGITAL_SILENCE) && recent.iter().any(|(mic, _)| *mic > MIC_ACTIVE)
}

/// Watch the recording: alert once if the recorded output stays silent (video calls
/// only; in-person meetings have no system sound).
pub fn watch_recording<R: Runtime>(app: AppHandle<R>, device: Option<String>) {
    let session = SESSION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        let mut last_len = 0usize;
        let mut idle_ticks = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if SESSION.load(Ordering::SeqCst) != session {
                return;
            }
            let timeline = super::source_timeline::snapshot();
            // Recording stopped (or paused for long): stop watching
            if timeline.windows.len() == last_len {
                idle_ticks += 1;
                if idle_ticks >= 24 {
                    return;
                }
                continue;
            }
            idle_ticks = 0;
            last_len = timeline.windows.len();
            let in_person = super::meeting_context::live_context()
                .and_then(|c| c.meeting_type)
                .is_some_and(|t| t == "presentiel");
            if in_person || !system_silent(&timeline.windows, timeline.window_secs) {
                continue;
            }
            let alternative = platform::default_outputs()
                .map(|(media, comms)| if Some(&comms.name) == device.as_ref() { media.name } else { comms.name })
                .filter(|name| Some(name) != device.as_ref());
            warn!("🔇 No system audio for {}s on {:?} (alternative: {:?})", SILENT_ALERT_SECS, device, alternative);
            let _ = app.emit("system-audio-silent", serde_json::json!({ "device": device, "alternative": alternative }));
            return;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(id: &str, peak: f32) -> Output {
        Output { id: id.into(), name: format!("{id} name"), peak }
    }

    #[test]
    fn records_where_the_meeting_plays() {
        // Same device for both roles: upstream behaviour
        assert_eq!(pick(&out("a", 0.0), &out("a", 0.0)), None);
        // Teams in the headset, nothing on the speakers
        assert_eq!(pick(&out("speakers", 0.0), &out("headset", 0.2)), Some("headset name".into()));
        // Browser meeting on the speakers, headset idle
        assert_eq!(pick(&out("speakers", 0.3), &out("headset", 0.0)), None);
        // Nothing playing yet: calls will be heard on the communications output
        assert_eq!(pick(&out("speakers", 0.0), &out("headset", 0.0)), Some("headset name".into()));
        // Both playing: the call wins
        assert_eq!(pick(&out("speakers", 0.3), &out("headset", 0.2)), Some("headset name".into()));
    }

    /// Reads the real Windows outputs (no audio is recorded): cargo test -- --ignored
    #[test]
    #[ignore]
    fn live_windows_default_outputs() {
        let outputs = platform::default_outputs().expect("Windows outputs");
        println!("multimedia: {:?}
communications: {:?}", outputs.0, outputs.1);
        println!("recorded: {:?}", meeting_output());
    }

    #[test]
    fn alerts_only_after_30s_of_digital_silence_with_speech() {
        let window = 0.6;
        let silent_with_speech: Vec<(f32, f32)> = (0..60).map(|i| (if i % 3 == 0 { 1e-3 } else { 0.0 }, 0.0)).collect();
        assert!(system_silent(&silent_with_speech, window));
        assert!(!system_silent(&silent_with_speech[..40], window), "not yet 30 s");
        let nobody_speaks: Vec<(f32, f32)> = (0..60).map(|_| (0.0, 0.0)).collect();
        assert!(!system_silent(&nobody_speaks, window));
        let mut some_sound = silent_with_speech.clone();
        some_sound[55].1 = 1e-6;
        assert!(!system_silent(&some_sound, window), "quiet but not digital silence");
    }
}

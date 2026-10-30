'use client';

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { AlertTriangle, Mic, Monitor, Volume2 } from 'lucide-react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { audioLevels, listMonitors, monitorThumbnails, type MonitorInfo } from '@/services/forkService';
import { autoOutput, effectiveDevice, levelPercent, PeakHistory, PLAYING_PEAK, screenChoiceKey, type AudioLevels } from '@/lib/audio-sources';

export interface RecordingSources {
  /** "<name> (input)", or null for the Windows default */
  micDevice: string | null;
  /** "<name> (output)" for this meeting only, or null for Automatique */
  systemDevice: string | null;
  /** "primary", "all" or "pos:x,y" */
  screenMonitor: string;
}

interface AudioDevice {
  name: string;
  device_type: 'Input' | 'Output';
}

const DEFAULT = '__default__';
const SILENT_HINT_MS = 3000;

const strip = (device: string) => device.replace(/ \((input|output)\)$/, '');

function Gauge({ peak }: { peak: number }) {
  return (
    <div className="h-1.5 w-20 shrink-0 overflow-hidden rounded-full bg-gray-200" aria-hidden>
      <div className="h-full rounded-full bg-green-500 transition-[width] duration-200" style={{ width: `${levelPercent(peak)}%` }} />
    </div>
  );
}

/**
 * Microphone, meeting audio and captured screen(s), chosen when a meeting starts
 * (Epic 7). Shows the device that will really be recorded with a live gauge, never
 * sends an unplugged device, and identifies screens by thumbnail and position.
 */
export function RecordingSourcesPicker({ value, onChange, captureScreen, meetingType }: {
  value: RecordingSources;
  onChange: (value: RecordingSources) => void;
  captureScreen: boolean;
  meetingType?: string | null;
}) {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<AudioDevice[] | null>(null);
  const [monitors, setMonitors] = useState<MonitorInfo[] | null>(null);
  const [thumbnails, setThumbnails] = useState<Record<string, string>>({});
  const [levels, setLevels] = useState<AudioLevels | null>(null);
  const [missing, setMissing] = useState<{ mic?: string; system?: string; screen?: boolean }>({});
  const [showSystem, setShowSystem] = useState(false);
  const [silentSince, setSilentSince] = useState<number | null>(null);
  const history = useRef(new PeakHistory());
  const valueRef = useRef(value);
  valueRef.current = value;

  useEffect(() => {
    invoke<AudioDevice[]>('get_audio_devices').then(setDevices).catch(error => {
      console.warn('Could not list audio devices:', error);
      setDevices([]);
    });
    listMonitors().then(setMonitors).catch(() => setMonitors([]));
  }, []);

  useEffect(() => {
    if (!captureScreen || Object.keys(thumbnails).length) return;
    monitorThumbnails()
      .then(list => setThumbnails(Object.fromEntries(list.map(m => [m.key, m.dataUrl]))))
      .catch(error => console.warn('Could not capture screen thumbnails:', error));
  }, [captureScreen, thumbnails]);

  // Live levels while the dialog is open (Windows peak meters, no audio stream)
  useEffect(() => {
    let stopped = false;
    const tick = async () => {
      try {
        const next = await audioLevels();
        if (stopped) return;
        history.current.add([...next.outputs, ...next.inputs], Date.now());
        setLevels(next);
      } catch {
        // gauges are optional
      }
    };
    void tick();
    const timer = window.setInterval(tick, 300);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, []);

  const inputs = devices?.filter(d => d.device_type === 'Input').map(d => `${d.name} (input)`) ?? null;
  const outputs = devices?.filter(d => d.device_type === 'Output').map(d => `${d.name} (output)`) ?? null;

  // A remembered device or screen that is gone is replaced (and reported), never sent as is
  useEffect(() => {
    if (inputs === null || outputs === null) return;
    const mic = effectiveDevice(valueRef.current.micDevice, inputs);
    const system = effectiveDevice(valueRef.current.systemDevice, outputs);
    if (mic.missing || system.missing) {
      setMissing(m => ({ ...m, mic: mic.missing ? strip(mic.missing) : m.mic, system: system.missing ? strip(system.missing) : m.system }));
      onChange({ ...valueRef.current, micDevice: mic.value, systemDevice: system.value });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [devices]);

  useEffect(() => {
    if (monitors === null) return;
    const screen = screenChoiceKey(valueRef.current.screenMonitor, monitors);
    if (screen.key !== valueRef.current.screenMonitor) {
      if (screen.missing) setMissing(m => ({ ...m, screen: true }));
      onChange({ ...valueRef.current, screenMonitor: screen.key });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [monitors]);

  const auto = levels ? autoOutput(levels.outputs, history.current.recent) : null;
  const recordedOutput = value.systemDevice ? strip(value.systemDevice) : auto;
  const recordedMic = value.micDevice ? strip(value.micDevice) : levels?.inputs.find(i => i.isDefault)?.name ?? null;
  const outputPeak = recordedOutput ? history.current.recent(recordedOutput) : 0;
  const micPeak = recordedMic ? history.current.recent(recordedMic) : 0;

  useEffect(() => {
    if (!levels) return;
    if (outputPeak > PLAYING_PEAK) setSilentSince(null);
    else setSilentSince(since => since ?? Date.now());
  }, [levels, outputPeak]);

  const inPerson = meetingType === 'presentiel';
  const systemFolded = inPerson && !showSystem && !value.systemDevice;
  const silentTooLong = !inPerson && silentSince !== null && Date.now() - silentSince > SILENT_HINT_MS;
  const primaryKey = monitors?.find(m => m.primary)?.key;
  const selectedScreen = value.screenMonitor === 'primary' ? primaryKey : value.screenMonitor;

  return (
    <div className="space-y-3 rounded-md border border-gray-200 p-3">
      <div className="text-sm font-medium text-gray-800">{t('fork.sources.title')}</div>

      <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-2 text-sm">
        <span className="flex items-center gap-1.5 whitespace-nowrap text-gray-600">
          <Mic className="h-4 w-4" /> {t('fork.sources.mic')}
        </span>
        <Select value={value.micDevice ?? DEFAULT} onValueChange={v => onChange({ ...value, micDevice: v === DEFAULT ? null : v })}>
          <SelectTrigger className="h-9 min-w-0 [&>span]:truncate">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={DEFAULT}>
              {recordedMic && !value.micDevice ? t('fork.sources.micDefaultNamed', { device: recordedMic }) : t('fork.sources.micDefault')}
            </SelectItem>
            {(inputs ?? []).map(d => (
              <SelectItem key={d} value={d}>{strip(d)}</SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Gauge peak={micPeak} />

        <span className="flex items-center gap-1.5 whitespace-nowrap text-gray-600">
          <Volume2 className="h-4 w-4" /> {t('fork.sources.system')}
        </span>
        {systemFolded ? (
          <span className="text-xs text-gray-500">
            {t('fork.sources.inPerson')}{' '}
            <button type="button" className="text-blue-600 hover:underline" onClick={() => setShowSystem(true)}>
              {t('fork.sources.edit')}
            </button>
          </span>
        ) : (
          <Select value={value.systemDevice ?? DEFAULT} onValueChange={v => onChange({ ...value, systemDevice: v === DEFAULT ? null : v })}>
            <SelectTrigger className="h-9 min-w-0 [&>span]:truncate">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={DEFAULT}>{auto ? t('fork.sources.systemAutoNamed', { device: auto }) : t('fork.sources.systemAuto')}</SelectItem>
              {(outputs ?? []).map(d => (
                <SelectItem key={d} value={d}>{strip(d)}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        {systemFolded ? <span /> : <Gauge peak={outputPeak} />}
      </div>

      {silentTooLong && !systemFolded && <p className="text-xs text-gray-500">{t('fork.sources.silentHint')}</p>}
      {value.systemDevice && <p className="text-xs text-gray-500">{t('fork.sources.systemThisMeeting')}</p>}
      {missing.mic && (
        <p className="flex items-center gap-1 text-xs text-amber-700">
          <AlertTriangle className="h-3.5 w-3.5" /> {t('fork.sources.missingMic', { device: missing.mic })}
        </p>
      )}
      {missing.system && (
        <p className="flex items-center gap-1 text-xs text-amber-700">
          <AlertTriangle className="h-3.5 w-3.5" /> {t('fork.sources.missingSystem', { device: missing.system })}
        </p>
      )}

      {captureScreen && monitors && monitors.length > 0 && (
        <div className="space-y-2">
          <div className="flex items-center gap-1.5 text-sm text-gray-600">
            <Monitor className="h-4 w-4" /> {t('fork.sources.screen')}
          </div>
          <div className="flex flex-wrap gap-2">
            {monitors.length > 1 && (
              <button
                type="button"
                onClick={() => onChange({ ...value, screenMonitor: 'all' })}
                className={`flex h-[104px] w-32 items-center justify-center rounded-md border-2 px-2 text-center text-xs font-medium ${
                  value.screenMonitor === 'all' ? 'border-blue-600 bg-blue-50 text-blue-800' : 'border-gray-200 text-gray-700 hover:border-gray-300'
                }`}
              >
                {t('fork.cloud.allMonitors', { count: monitors.length })}
              </button>
            )}
            {monitors.map(m => (
              <button
                key={m.key}
                type="button"
                onClick={() => onChange({ ...value, screenMonitor: m.key })}
                title={[m.name, `${m.width}×${m.height}`].filter(Boolean).join(' — ')}
                className={`w-32 overflow-hidden rounded-md border-2 text-left ${
                  selectedScreen === m.key ? 'border-blue-600 bg-blue-50' : 'border-gray-200 hover:border-gray-300'
                }`}
              >
                <div className="flex h-[72px] items-center justify-center bg-gray-100">
                  {thumbnails[m.key] ? (
                    <img src={thumbnails[m.key]} alt="" className="h-full w-full object-cover" />
                  ) : (
                    <Monitor className="h-6 w-6 text-gray-300" />
                  )}
                </div>
                <div className="px-1.5 py-1 text-[11px] leading-tight text-gray-700">
                  <span className="font-medium">{t('fork.sources.screenNumber', { n: m.number })}</span>
                  {m.primary ? ` · ${t('fork.cloud.primary')}` : ''}
                  <span className="block text-gray-400">{m.width}×{m.height}</span>
                </div>
              </button>
            ))}
          </div>
          {missing.screen && (
            <p className="flex items-center gap-1 text-xs text-amber-700">
              <AlertTriangle className="h-3.5 w-3.5" /> {t('fork.sources.missingScreen')}
            </p>
          )}
        </div>
      )}

      <p className="text-xs text-gray-500">{t('fork.sources.hint')}</p>
    </div>
  );
}

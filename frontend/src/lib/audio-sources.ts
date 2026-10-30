// Meetily perso: recording sources chosen when a meeting starts (pure helpers).

export interface EndpointLevel {
  /** Friendly name, as listed by get_audio_devices */
  name: string;
  peak: number;
  /** Default multimedia output / default microphone */
  isDefault: boolean;
  /** Default communications output (where calls play) */
  isCommunications: boolean;
}

export interface AudioLevels {
  outputs: EndpointLevel[];
  inputs: EndpointLevel[];
}

/** Peak (0..1) above which an output is considered playing (same as the backend). */
export const PLAYING_PEAK = 0.001;

/**
 * Output that "Automatique" records, with the backend rule (fork/audio_route.rs `pick`)
 * applied to recent peaks: the default output if it alone plays, else the communications
 * output (where calls are heard).
 */
export function autoOutput(outputs: EndpointLevel[], recentPeak: (name: string) => number): string | null {
  const media = outputs.find(o => o.isDefault);
  const comms = outputs.find(o => o.isCommunications);
  if (!media || !comms || media.name === comms.name) return (media ?? comms)?.name ?? null;
  const mediaPlaying = recentPeak(media.name) > PLAYING_PEAK;
  const commsPlaying = recentPeak(comms.name) > PLAYING_PEAK;
  return mediaPlaying && !commsPlaying ? media.name : comms.name;
}

/** Gauge width (0..100) on a decibel scale: -60 dB and below is empty, 0 dB is full. */
export function levelPercent(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(peak);
  return Math.max(0, Math.min(100, ((db + 60) / 60) * 100));
}

/**
 * Device actually sent at start: a remembered device that is not plugged in becomes
 * null (default / automatic) and is reported, so it is never sent as is.
 */
export function effectiveDevice(device: string | null, available: string[] | null): { value: string | null; missing: string | null } {
  if (!device || available === null) return { value: device, missing: null };
  return available.includes(device) ? { value: device, missing: null } : { value: null, missing: device };
}

/**
 * Screen choice as a durable key: "all", "primary" or "pos:x,y". The former
 * "monitor:<id>" values are mapped to the screen's position; a screen that is gone
 * becomes "primary" and is reported.
 */
export function screenChoiceKey(choice: string, monitors: { id: string; key: string }[] | null): { key: string; missing: boolean } {
  if (choice === 'all' || choice === 'primary' || !choice) return { key: choice || 'primary', missing: false };
  if (monitors === null) return { key: choice, missing: false };
  const found = monitors.find(m => m.key === choice || m.id === choice);
  return found ? { key: found.key, missing: false } : { key: 'primary', missing: true };
}

/** Rolling maximum of each device's peak over the last `windowMs`. */
export class PeakHistory {
  private samples = new Map<string, { t: number; peak: number }[]>();

  constructor(private windowMs = 1500) {}

  add(levels: EndpointLevel[], now: number) {
    for (const level of levels) {
      const list = this.samples.get(level.name) ?? [];
      list.push({ t: now, peak: level.peak });
      while (list.length && list[0].t < now - this.windowMs) list.shift();
      this.samples.set(level.name, list);
    }
  }

  recent = (name: string) => Math.max(0, ...(this.samples.get(name) ?? []).map(s => s.peak));
}

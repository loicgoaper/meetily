// Meetily perso: display labels for transcript speakers.
//
// Raw keys stored with each segment:
//   'mic'        live: the person recording (microphone side)
//   'system'     live: remote participants (computer audio)
//   'mixed'      live: both at once, no label shown
//   'speaker_N'  after end-of-meeting identification
// Names come from meeting_speakers (automatic naming or manual rename).

import type { TFunction } from 'i18next';
import type { MeetingSpeaker } from '@/services/forkService';

export type SpeakerMap = Record<string, MeetingSpeaker>;

export function toSpeakerMap(speakers: MeetingSpeaker[]): SpeakerMap {
  return Object.fromEntries(speakers.map(s => [s.speakerKey, s]));
}

/** Human label for a raw speaker key, or null when no label should be shown. */
export function speakerLabel(key: string | null | undefined, speakers: SpeakerMap, t: TFunction): string | null {
  if (!key || key === 'mixed') return null;
  const known = speakers[key];
  if (known?.displayName) return known.displayName;
  if (key === 'mic' || known?.source === 'mic-anchor') return t('fork.speakers.me');
  if (key === 'system') return t('fork.speakers.remote');
  const match = /^speaker_(\d+)$/.exec(key);
  if (match) return t('fork.speakers.numbered', { n: Number(match[1]) + 1 });
  return key;
}

const PALETTE = [
  'bg-blue-100 text-blue-800',
  'bg-emerald-100 text-emerald-800',
  'bg-amber-100 text-amber-800',
  'bg-purple-100 text-purple-800',
  'bg-rose-100 text-rose-800',
  'bg-cyan-100 text-cyan-800',
  'bg-lime-100 text-lime-800',
  'bg-orange-100 text-orange-800',
];

/** Stable chip colour per speaker key. */
export function speakerColor(key: string): string {
  if (key === 'mic') return PALETTE[0];
  if (key === 'system') return PALETTE[1];
  const match = /^speaker_(\d+)$/.exec(key);
  const index = match ? Number(match[1]) : [...key].reduce((acc, c) => acc + c.charCodeAt(0), 0);
  return PALETTE[index % PALETTE.length];
}

/**
 * Transcript lines for the summary model: "[mm:ss] Name : text". The label is written
 * when the speaker changes; an unlabelled line continues the previous speaker.
 */
export function formatTranscriptForSummary(
  lines: { text: string; audio_start_time?: number; timestamp: string; speaker?: string | null }[],
  speakers: SpeakerMap,
  t: TFunction,
): string {
  let lastLabel: string | null = null;
  return lines
    .map(line => {
      const seconds = line.audio_start_time;
      const time = seconds === undefined
        ? line.timestamp
        : `[${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(Math.floor(seconds) % 60).padStart(2, '0')}]`;
      const label = speakerLabel(line.speaker, speakers, t);
      const showLabel = label !== null && label !== lastLabel;
      lastLabel = label;
      return showLabel ? `${time} ${label} : ${line.text}` : `${time} ${line.text}`;
    })
    .join('\n');
}

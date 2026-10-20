// Meetily perso: typed wrappers around the fork's Tauri commands (src-tauri/src/fork).

import { invoke } from '@tauri-apps/api/core';

// ---------------------------------------------------------------------------
// Preferences
// ---------------------------------------------------------------------------

export const PREF = {
  sttLiveModel: 'stt.live_model',
  sttLanguage: 'stt.language',
  sttFallbackLocal: 'stt.fallback_local',
  sttConcurrency: 'stt.concurrency',
  privacyZdr: 'privacy.zdr',
  diarizationAuto: 'diarization.auto',
  diarizationModel: 'diarization.model',
  namingModel: 'naming.model',
  visionModel: 'vision.model',
  screenInterval: 'screen.interval_seconds',
  screenDefaultEnabled: 'screen.default_enabled',
  screenMonitor: 'screen.monitor',
  summaryDefaultTemplate: 'summary.default_template',
  uiLanguage: 'ui.language',
} as const;

export type Preferences = Record<string, string>;

export const getPreferences = () => invoke<Preferences>('api_get_app_preferences');
export const setPreferences = (values: Preferences) => invoke<void>('api_set_app_preferences', { values });

// ---------------------------------------------------------------------------
// Meeting context and participants
// ---------------------------------------------------------------------------

export type MeetingType = 'presentiel' | 'visio';

export interface Participant {
  id?: string | null;
  name: string;
  role?: string | null;
  company?: string | null;
  isSelf?: boolean;
}

export interface MeetingContext {
  contextText?: string | null;
  glossary?: string | null;
  meetingType?: MeetingType | null;
  extraInstructions?: string | null;
  participants: Participant[];
  /** Live only: capture the screen during this recording */
  captureScreen?: boolean | null;
}

export interface Contact {
  id: string;
  name: string;
  role?: string | null;
  company?: string | null;
}

export const emptyContext = (): MeetingContext => ({ participants: [] });

export const getMeetingContext = (meetingId: string) =>
  invoke<MeetingContext>('api_get_meeting_context', { meetingId });
export const saveMeetingContext = (meetingId: string, context: MeetingContext) =>
  invoke<void>('api_save_meeting_context', { meetingId, context });
export const setLiveMeetingContext = (context: MeetingContext | null) =>
  invoke<void>('api_set_live_meeting_context', { context });
export const getLiveMeetingContext = () => invoke<MeetingContext | null>('api_get_live_meeting_context');
export const searchContacts = (query: string) => invoke<Contact[]>('api_search_contacts', { query });

// ---------------------------------------------------------------------------
// Cloud transcription
// ---------------------------------------------------------------------------

export interface SttModelInfo {
  id: string;
  name: string;
  pricePerSecond?: number | null;
}

export const getTranscriptionModels = () => invoke<SttModelInfo[]>('get_openrouter_transcription_models');

export interface KeyStatus {
  configured: boolean;
  masked?: string | null;
}

/** The single OpenRouter key (summary, transcription, speakers, vision). */
export const getOpenRouterKeyStatus = () => invoke<KeyStatus>('api_get_openrouter_key_status');
export const setOpenRouterKey = (key: string) => invoke<KeyStatus>('api_set_openrouter_key', { key });

/** Save the live transcription engine choice (same command as the local model managers). */
export const saveTranscriptConfig = (provider: string, model: string) =>
  invoke<void>('api_save_transcript_config', { provider, model, apiKey: null });

export interface UsageRow {
  kind: 'stt_live' | 'diarization' | 'naming' | 'vision' | string;
  model?: string | null;
  audioSeconds?: number | null;
  costUsd?: number | null;
  createdAt: string;
}

export const getMeetingUsage = (meetingId: string) => invoke<UsageRow[]>('api_get_meeting_usage', { meetingId });

// ---------------------------------------------------------------------------
// Speakers and diarization
// ---------------------------------------------------------------------------

export interface MeetingSpeaker {
  speakerKey: string;
  displayName?: string | null;
  participantId?: string | null;
  source: 'auto' | 'manual' | 'mic-anchor' | string;
  confidence?: number | null;
}

export interface DiarizationResult {
  meetingId: string;
  segments: number;
  speakers: number;
  named: number;
  costUsd: number;
}

export const getMeetingSpeakers = (meetingId: string) =>
  invoke<MeetingSpeaker[]>('api_get_meeting_speakers', { meetingId });
export const renameSpeaker = (meetingId: string, speakerKey: string, displayName: string | null) =>
  invoke<void>('api_rename_speaker', { meetingId, speakerKey, displayName });
export const setSegmentSpeaker = (transcriptId: string, speakerKey: string | null) =>
  invoke<void>('api_set_segment_speaker', { transcriptId, speakerKey });
export const diarizeMeeting = (meetingId: string) => invoke<DiarizationResult>('api_diarize_meeting', { meetingId });
export const cancelDiarization = () => invoke<void>('api_cancel_diarization');
export const isDiarizationInProgress = () => invoke<boolean>('api_is_diarization_in_progress');
export const revertDiarization = (meetingId: string) => invoke<void>('api_revert_diarization', { meetingId });
export const hasLiveTranscriptBackup = (meetingId: string) =>
  invoke<boolean>('api_has_live_transcript_backup', { meetingId });

// ---------------------------------------------------------------------------
// Screen capture and vision
// ---------------------------------------------------------------------------

export interface MonitorInfo {
  id: string;
  name: string;
  primary: boolean;
  width: number;
  height: number;
}

export interface ScreenFrame {
  file: string;
  t: number;
  excluded: boolean;
  change: number;
}

export interface ScreenNote {
  t: number;
  file: string;
  title: string;
  content: string;
}

export interface VisionResult {
  frames: number;
  notes: number;
  costUsd: number;
  cached: boolean;
}

export const listMonitors = () => invoke<MonitorInfo[]>('api_list_monitors');
export const startScreenCapture = () => invoke<void>('api_start_screen_capture');
export const stopScreenCapture = () => invoke<void>('api_stop_screen_capture');
export const getScreenCaptureStatus = () => invoke<{ active: boolean; frames: number }>('api_screen_capture_status');
export const listMeetingScreenshots = (meetingId: string) =>
  invoke<ScreenFrame[]>('api_list_meeting_screenshots', { meetingId });
export const getScreenshot = (meetingId: string, file: string, thumbnail: boolean) =>
  invoke<string>('api_get_screenshot', { meetingId, file, thumbnail });
export const setScreenshotExcluded = (meetingId: string, file: string, excluded: boolean) =>
  invoke<void>('api_set_screenshot_excluded', { meetingId, file, excluded });
export const analyzeScreenshots = (meetingId: string, force = false) =>
  invoke<VisionResult>('api_analyze_screenshots', { meetingId, force });
export const getScreenNotes = (meetingId: string) => invoke<ScreenNote[]>('api_get_screen_notes', { meetingId });

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

export type TemplateFormat = 'paragraph' | 'list' | 'string';

export interface TemplateSection {
  title: string;
  instruction: string;
  format: TemplateFormat;
  item_format?: string | null;
  example_item_format?: string | null;
}

export interface Template {
  name: string;
  description: string;
  sections: TemplateSection[];
}

export interface TemplateEntry {
  id: string;
  name: string;
  description: string;
  source: 'custom' | 'bundled' | 'builtin';
  overridesOriginal: boolean;
  sections: number;
}

export interface TemplateFull {
  id: string;
  source: 'custom' | 'bundled' | 'builtin';
  overridesOriginal: boolean;
  template: Template;
}

export const listTemplatesFull = () => invoke<TemplateEntry[]>('api_list_templates_full');
export const getTemplateFull = (id: string) => invoke<TemplateFull>('api_get_template_full', { id });
export const saveCustomTemplate = (id: string | null, template: Template) =>
  invoke<string>('api_save_custom_template', { id, template });
export const deleteCustomTemplate = (id: string) => invoke<boolean>('api_delete_custom_template', { id });
export const duplicateTemplate = (id: string, newName: string) =>
  invoke<string>('api_duplicate_template', { id, newName });
/** Opens a file picker; resolves to the new template id, or null if cancelled. */
export const importTemplate = () => invoke<string | null>('api_import_template');
/** Opens a save dialog; resolves to false if cancelled. */
export const exportTemplate = (id: string) => invoke<boolean>('api_export_template', { id });
export const previewTemplate = (template: Template) =>
  invoke<{ markdown: string; instructions: string; valid: string | null }>('api_preview_template', { template });

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export function formatTimestamp(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const mm = String(Math.floor(total / 60)).padStart(2, '0');
  const ss = String(total % 60).padStart(2, '0');
  return `${mm}:${ss}`;
}

export function formatUsd(value: number): string {
  return value < 0.01 ? `${value.toFixed(4)} $` : `${value.toFixed(2)} $`;
}

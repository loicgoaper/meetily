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
  /** Screen number (from 1) when every screen is captured. */
  monitor?: number | null;
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

// ---------------------------------------------------------------------------
// Lot 2: speakers merge, audio clips, export, tags, Notion, AI search
// ---------------------------------------------------------------------------

export const mergeSpeakers = (meetingId: string, fromKey: string, intoKey: string) =>
  invoke<number>('api_merge_speakers', { meetingId, fromKey, intoKey });

/** A passage of the meeting audio as an mp3 data URL. */
export const getAudioClip = (meetingId: string, start: number, end: number) =>
  invoke<string>('api_get_audio_clip', { meetingId, start, end });

export interface ExportParts {
  audio: boolean;
  transcript: boolean;
  summary: boolean;
  context: boolean;
  screenshots: boolean;
}

/** Opens a save dialog; resolves to the saved path, or null if cancelled. */
export const exportMeeting = (meetingId: string, parts: ExportParts) =>
  invoke<string | null>('api_export_meeting', { meetingId, parts });

/** Notion colour names (also used for tags in the app). */
export const TAG_COLORS = ['default', 'gray', 'brown', 'orange', 'yellow', 'green', 'blue', 'purple', 'pink', 'red'] as const;
export type TagColor = (typeof TAG_COLORS)[number];

/** Tailwind classes matching the Notion colours. */
export const TAG_COLOR_CLASSES: Record<string, { chip: string; dot: string }> = {
  default: { chip: 'bg-gray-100 text-gray-700 border-gray-200', dot: 'bg-gray-400' },
  gray: { chip: 'bg-gray-200 text-gray-800 border-gray-300', dot: 'bg-gray-500' },
  brown: { chip: 'bg-amber-100 text-amber-900 border-amber-200', dot: 'bg-amber-700' },
  orange: { chip: 'bg-orange-100 text-orange-800 border-orange-200', dot: 'bg-orange-500' },
  yellow: { chip: 'bg-yellow-100 text-yellow-800 border-yellow-200', dot: 'bg-yellow-400' },
  green: { chip: 'bg-green-100 text-green-800 border-green-200', dot: 'bg-green-500' },
  blue: { chip: 'bg-blue-100 text-blue-800 border-blue-200', dot: 'bg-blue-500' },
  purple: { chip: 'bg-purple-100 text-purple-800 border-purple-200', dot: 'bg-purple-500' },
  pink: { chip: 'bg-pink-100 text-pink-800 border-pink-200', dot: 'bg-pink-500' },
  red: { chip: 'bg-red-100 text-red-800 border-red-200', dot: 'bg-red-500' },
};

export interface Tag {
  id: string;
  name: string;
  color: TagColor | string;
}

export const listTags = () => invoke<Tag[]>('api_list_tags');
export const createTag = (name: string, color: string) => invoke<Tag>('api_create_tag', { name, color });
export const updateTag = (id: string, name: string, color: string) => invoke<void>('api_update_tag', { id, name, color });
export const deleteTag = (id: string) => invoke<void>('api_delete_tag', { id });
export const getMeetingTags = (meetingId: string) => invoke<Tag[]>('api_get_meeting_tags', { meetingId });
export const setMeetingTags = (meetingId: string, tagIds: string[]) => invoke<void>('api_set_meeting_tags', { meetingId, tagIds });
export const getAllMeetingTags = () => invoke<Record<string, string[]>>('api_get_all_meeting_tags');

/** Fired on window when tags or their assignments change (sidebar refresh). */
export const TAGS_CHANGED_EVENT = 'fork-tags-changed';
export const notifyTagsChanged = () => window.dispatchEvent(new Event(TAGS_CHANGED_EVENT));

export interface NotionStatus {
  hasToken: boolean;
  tokenMasked?: string | null;
  workspace?: string | null;
  integration?: string | null;
  parentPageId?: string | null;
  databaseId?: string | null;
  databaseUrl?: string | null;
  autoSync: boolean;
  pending: number;
  error?: string | null;
}

export interface NotionPage {
  id: string;
  title: string;
  url: string;
}

export interface NotionMeetingState {
  pageUrl?: string | null;
  syncedAt?: string | null;
  lastError?: string | null;
  pending: boolean;
}

export const getNotionStatus = () => invoke<NotionStatus>('api_notion_status');
export const setNotionToken = (token: string) => invoke<void>('api_notion_set_token', { token });
export const listNotionPages = () => invoke<NotionPage[]>('api_notion_list_pages');
export const createNotionDatabase = (parentPageId: string) => invoke<string>('api_notion_create_database', { parentPageId });
export const syncMeetingToNotion = (meetingId: string) => invoke<string>('api_notion_sync_meeting', { meetingId });
export const syncAllToNotion = () => invoke<number>('api_notion_sync_all');
export const getNotionMeetingState = (meetingId: string) => invoke<NotionMeetingState>('api_notion_meeting_state', { meetingId });

export interface AiSearchResult {
  meetingId: string;
  title: string;
  date: string;
  score: number;
  reason: string;
  hits: { t?: number | null; text: string }[];
}

export const aiSearch = (query: string) => invoke<AiSearchResult[]>('api_ai_search', { query });

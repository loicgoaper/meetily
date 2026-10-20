// Meetily perso: interface translations (FR/EN).
//
// One JSON file per area of the interface (src/i18n/locales/<lang>/<area>.json),
// merged under a top-level key: t('settings.tabs.general'), t('meeting.copy')...
// French is the default; English is the fallback for any missing key.

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import frCommon from './locales/fr/common.json';
import frFork from './locales/fr/fork.json';
import frHome from './locales/fr/home.json';
import frRecording from './locales/fr/recording.json';
import frSidebar from './locales/fr/sidebar.json';
import frSettings from './locales/fr/settings.json';
import frModels from './locales/fr/models.json';
import frMeeting from './locales/fr/meeting.json';
import frSummary from './locales/fr/summary.json';
import frOnboarding from './locales/fr/onboarding.json';
import frImportAudio from './locales/fr/importAudio.json';
import frMisc from './locales/fr/misc.json';

import enCommon from './locales/en/common.json';
import enFork from './locales/en/fork.json';
import enHome from './locales/en/home.json';
import enRecording from './locales/en/recording.json';
import enSidebar from './locales/en/sidebar.json';
import enSettings from './locales/en/settings.json';
import enModels from './locales/en/models.json';
import enMeeting from './locales/en/meeting.json';
import enSummary from './locales/en/summary.json';
import enOnboarding from './locales/en/onboarding.json';
import enImportAudio from './locales/en/importAudio.json';
import enMisc from './locales/en/misc.json';

export type UiLanguage = 'fr' | 'en';
export const UI_LANGUAGES: UiLanguage[] = ['fr', 'en'];
export const UI_LANGUAGE_STORAGE_KEY = 'ui.language';

const resources = {
  fr: {
    translation: {
      common: frCommon,
      fork: frFork,
      home: frHome,
      recording: frRecording,
      sidebar: frSidebar,
      settings: frSettings,
      models: frModels,
      meeting: frMeeting,
      summary: frSummary,
      onboarding: frOnboarding,
      importAudio: frImportAudio,
      misc: frMisc,
    },
  },
  en: {
    translation: {
      common: enCommon,
      fork: enFork,
      home: enHome,
      recording: enRecording,
      sidebar: enSidebar,
      settings: enSettings,
      models: enModels,
      meeting: enMeeting,
      summary: enSummary,
      onboarding: enOnboarding,
      importAudio: enImportAudio,
      misc: enMisc,
    },
  },
};

/** Language saved on this device (fast path, avoids a flash of the wrong language). */
export function getStoredUiLanguage(): UiLanguage {
  try {
    const stored = typeof window !== 'undefined' ? window.localStorage.getItem(UI_LANGUAGE_STORAGE_KEY) : null;
    if (stored === 'fr' || stored === 'en') return stored;
  } catch {
    // storage unavailable: use the default below
  }
  return 'fr';
}

if (!i18n.isInitialized) {
  i18n.use(initReactI18next).init({
    resources,
    lng: getStoredUiLanguage(),
    fallbackLng: 'en',
    interpolation: { escapeValue: false },
    returnNull: false,
  });
}

/** Locale tag for dates and numbers (Intl / toLocaleString). */
export function currentLocale(): string {
  return i18n.language === 'en' ? 'en-US' : 'fr-CA';
}

export default i18n;

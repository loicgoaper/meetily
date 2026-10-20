'use client';

import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react';
import { I18nextProvider } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import i18n, { getStoredUiLanguage, UI_LANGUAGE_STORAGE_KEY, type UiLanguage } from './index';

interface UiLanguageContextValue {
  language: UiLanguage;
  setLanguage: (language: UiLanguage) => Promise<void>;
}

const UiLanguageContext = createContext<UiLanguageContextValue>({
  language: 'fr',
  setLanguage: async () => {},
});

export function useUiLanguage() {
  return useContext(UiLanguageContext);
}

function applyLanguage(language: UiLanguage) {
  if (i18n.language !== language) {
    void i18n.changeLanguage(language);
  }
  try {
    window.localStorage.setItem(UI_LANGUAGE_STORAGE_KEY, language);
  } catch {
    // per-device cache only; the saved preference lives in the database
  }
  document.documentElement.lang = language;
}

/**
 * Provides translations and keeps the UI language in sync with the saved preference
 * (app_preferences['ui.language'], also used by the Rust tray menu).
 */
export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<UiLanguage>(getStoredUiLanguage());

  useEffect(() => {
    document.documentElement.lang = language;
    invoke<Record<string, string>>('api_get_app_preferences')
      .then(prefs => {
        const saved = prefs?.['ui.language'];
        if ((saved === 'fr' || saved === 'en') && saved !== language) {
          applyLanguage(saved);
          setLanguageState(saved);
        }
      })
      .catch(err => console.warn('[i18n] Could not read saved UI language:', err));
    // run once at startup
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const setLanguage = useCallback(async (next: UiLanguage) => {
    applyLanguage(next);
    setLanguageState(next);
    await invoke('api_set_app_preferences', { values: { 'ui.language': next } });
  }, []);

  return (
    <UiLanguageContext.Provider value={{ language, setLanguage }}>
      <I18nextProvider i18n={i18n}>{children}</I18nextProvider>
    </UiLanguageContext.Provider>
  );
}

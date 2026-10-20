'use client';

import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import { getPreferences, setPreferences, type Preferences } from '@/services/forkService';

/** Fork preferences (app_preferences table) with optimistic updates. */
export function usePreferences() {
  const { t } = useTranslation();
  const [prefs, setPrefs] = useState<Preferences>({});
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    getPreferences()
      .then(values => setPrefs(values))
      .catch(error => console.error('Failed to load preferences:', error))
      .finally(() => setLoaded(true));
  }, []);

  const update = useCallback(
    async (values: Preferences) => {
      setPrefs(prev => ({ ...prev, ...values }));
      try {
        await setPreferences(values);
      } catch (error) {
        console.error('Failed to save preferences:', error);
        toast.error(t('fork.common.saveError'));
      }
    },
    [t],
  );

  const get = useCallback((key: string, fallback = '') => prefs[key] ?? fallback, [prefs]);
  const getBool = useCallback((key: string) => prefs[key] === 'true', [prefs]);

  return { prefs, loaded, get, getBool, update };
}

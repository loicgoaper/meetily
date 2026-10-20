'use client';

import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { useUiLanguage } from '@/i18n/I18nProvider';
import type { UiLanguage } from '@/i18n';
import { SettingsCard } from './SettingsCard';

/** Interface language (French / English), saved in the database. */
export function LanguageSettings() {
  const { t } = useTranslation();
  const { language, setLanguage } = useUiLanguage();

  return (
    <SettingsCard
      title={t('fork.language.title')}
      description={t('fork.language.description')}
      action={
        <Select
          value={language}
          onValueChange={async value => {
            try {
              await setLanguage(value as UiLanguage);
            } catch (error) {
              console.error('Failed to save UI language:', error);
              toast.error(t('fork.language.saveError'));
            }
          }}
        >
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="fr">Français</SelectItem>
            <SelectItem value="en">English</SelectItem>
          </SelectContent>
        </Select>
      }
    />
  );
}

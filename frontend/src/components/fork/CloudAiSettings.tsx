'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { usePreferences } from '@/hooks/fork/usePreferences';
import { listMonitors, PREF, type MonitorInfo } from '@/services/forkService';
import { OpenRouterKeyField } from './OpenRouterKeyField';
import { SettingRow, SettingsCard } from './SettingsCard';

const VISION_SUGGESTIONS = [
  'google/gemini-3.1-flash-lite',
  'google/gemini-3.5-flash-lite',
  'qwen/qwen3.7-flash',
  'openai/gpt-5.6-luna',
  'anthropic/claude-haiku-5.5',
];

const NAMING_SUGGESTIONS = [
  'google/gemini-3.1-flash-lite',
  'anthropic/claude-haiku-5.5',
  'openai/gpt-6-luna',
  'qwen/qwen3.7-flash',
];

/** Model id typed freely, with suggestions; saved when the field loses focus. */
function ModelInput({ value, suggestions, listId, onSave }: { value: string; suggestions: string[]; listId: string; onSave: (v: string) => void }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <>
      <Input
        className="w-72 font-mono text-xs"
        list={listId}
        value={draft}
        onChange={e => setDraft(e.target.value)}
        onBlur={() => draft.trim() && draft.trim() !== value && onSave(draft.trim())}
        onKeyDown={e => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
      />
      <datalist id={listId}>
        {suggestions.map(s => <option key={s} value={s} />)}
      </datalist>
    </>
  );
}

/** "Cloud & IA" settings tab: everything the fork sends to OpenRouter. */
export function CloudAiSettings() {
  const { t } = useTranslation();
  const { get, getBool, update, loaded } = usePreferences();
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);

  useEffect(() => {
    listMonitors().then(setMonitors).catch(error => console.warn('Could not list monitors:', error));
  }, []);

  if (!loaded) {
    return <div className="p-6 text-sm text-gray-500">{t('fork.common.loading')}</div>;
  }

  return (
    <div className="space-y-6">
      <SettingsCard title={t('fork.cloud.keyTitle')} description={t('fork.cloud.keyDescription')}>
        <OpenRouterKeyField />
      </SettingsCard>

      <SettingsCard title={t('fork.cloud.speakersTitle')} description={t('fork.cloud.speakersDescription')}>
        <SettingRow label={t('fork.cloud.speakersAuto')} hint={t('fork.cloud.speakersAutoHint')}>
          <Switch
            checked={getBool(PREF.diarizationAuto)}
            onCheckedChange={checked => update({ [PREF.diarizationAuto]: String(checked) })}
          />
        </SettingRow>
        <SettingRow label={t('fork.cloud.diarizationModel')} hint={t('fork.cloud.diarizationModelHint')}>
          <ModelInput
            listId="diarization-models"
            value={get(PREF.diarizationModel)}
            suggestions={['elevenlabs/scribe-v2']}
            onSave={v => update({ [PREF.diarizationModel]: v })}
          />
        </SettingRow>
        <SettingRow label={t('fork.cloud.namingModel')} hint={t('fork.cloud.namingModelHint')}>
          <ModelInput
            listId="naming-models"
            value={get(PREF.namingModel)}
            suggestions={NAMING_SUGGESTIONS}
            onSave={v => update({ [PREF.namingModel]: v })}
          />
        </SettingRow>
      </SettingsCard>

      <SettingsCard title={t('fork.cloud.screenTitle')} description={t('fork.cloud.screenDescription')}>
        <SettingRow label={t('fork.cloud.screenDefault')} hint={t('fork.cloud.screenDefaultHint')}>
          <Switch
            checked={getBool(PREF.screenDefaultEnabled)}
            onCheckedChange={checked => update({ [PREF.screenDefaultEnabled]: String(checked) })}
          />
        </SettingRow>
        <SettingRow label={t('fork.cloud.screenInterval')} hint={t('fork.cloud.screenIntervalHint')}>
          <Select value={get(PREF.screenInterval, '5')} onValueChange={v => update({ [PREF.screenInterval]: v })}>
            <SelectTrigger className="w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {['3', '5', '10', '15', '30'].map(s => (
                <SelectItem key={s} value={s}>{t('fork.cloud.seconds', { count: Number(s) })}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </SettingRow>
        <SettingRow label={t('fork.cloud.screenMonitor')}>
          <Select value={get(PREF.screenMonitor, 'primary') || 'primary'} onValueChange={v => update({ [PREF.screenMonitor]: v })}>
            <SelectTrigger className="w-72">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="primary">{t('fork.cloud.primaryMonitor')}</SelectItem>
              {monitors.map(m => (
                <SelectItem key={m.id} value={m.id}>
                  {m.name || m.id} ({m.width}×{m.height}){m.primary ? ` — ${t('fork.cloud.primary')}` : ''}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </SettingRow>
        <SettingRow label={t('fork.cloud.visionModel')} hint={t('fork.cloud.visionModelHint')}>
          <ModelInput
            listId="vision-models"
            value={get(PREF.visionModel)}
            suggestions={VISION_SUGGESTIONS}
            onSave={v => update({ [PREF.visionModel]: v })}
          />
        </SettingRow>
      </SettingsCard>

      <SettingsCard title={t('fork.cloud.privacyTitle')} description={t('fork.cloud.privacyDescription')}>
        <SettingRow label={t('fork.cloud.zdr')} hint={t('fork.cloud.zdrHint')}>
          <Switch checked={getBool(PREF.privacyZdr)} onCheckedChange={checked => update({ [PREF.privacyZdr]: String(checked) })} />
        </SettingRow>
      </SettingsCard>
    </div>
  );
}

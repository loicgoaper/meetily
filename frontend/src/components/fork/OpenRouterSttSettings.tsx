'use client';

import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Cloud } from 'lucide-react';
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { usePreferences } from '@/hooks/fork/usePreferences';
import { getTranscriptionModels, PREF, saveTranscriptConfig, type SttModelInfo } from '@/services/forkService';
import type { TranscriptModelProps } from '@/components/TranscriptSettings';
import { OpenRouterKeyField } from './OpenRouterKeyField';
import { SettingRow } from './SettingsCard';

/** Models measured on French audio (2026-10-20): latency for 10-30 s chunks, cost per hour. */
export const RECOMMENDED_STT_MODELS: { id: string; costPerHour: string; noteKey: string }[] = [
  { id: 'elevenlabs/scribe-v2', costPerHour: '0,22', noteKey: 'scribe' },
  { id: 'openai/gpt-4o-transcribe', costPerHour: '0,22', noteKey: 'gpt4o' },
  { id: 'deepgram/nova-3', costPerHour: '0,26', noteKey: 'nova' },
  { id: 'mistralai/voxtral-mini-transcribe-3-0', costPerHour: '0,21', noteKey: 'voxtral' },
  { id: 'openai/whisper-large-v3-turbo', costPerHour: '0,01', noteKey: 'whisperTurbo' },
];

export function OpenRouterSttSettings({
  transcriptModelConfig,
  setTranscriptModelConfig,
}: {
  transcriptModelConfig: TranscriptModelProps;
  setTranscriptModelConfig: (config: TranscriptModelProps) => void;
}) {
  const { t } = useTranslation();
  const { get, getBool, update, loaded } = usePreferences();
  const [models, setModels] = useState<SttModelInfo[]>([]);
  const isActive = transcriptModelConfig.provider === 'openrouter';
  const selectedModel = isActive && transcriptModelConfig.model ? transcriptModelConfig.model : get(PREF.sttLiveModel, 'elevenlabs/scribe-v2');

  useEffect(() => {
    getTranscriptionModels()
      .then(setModels)
      .catch(error => console.warn('Could not list OpenRouter transcription models:', error));
  }, []);

  const otherModels = useMemo(
    () => models.filter(m => !RECOMMENDED_STT_MODELS.some(r => r.id === m.id)),
    [models],
  );

  const chooseModel = async (model: string) => {
    try {
      await saveTranscriptConfig('openrouter', model);
      await update({ [PREF.sttLiveModel]: model });
      setTranscriptModelConfig({ provider: 'openrouter', model, apiKey: null });
      toast.success(t('fork.stt.saved', { model }));
    } catch (error) {
      console.error('Failed to save transcription model:', error);
      toast.error(t('fork.common.saveError'));
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex items-start gap-3 rounded-md bg-blue-50 p-3 text-sm text-blue-900">
        <Cloud className="w-5 h-5 mt-0.5 shrink-0" />
        <p>{t('fork.stt.intro')}</p>
      </div>

      <OpenRouterKeyField />

      <div className="space-y-2">
        <div className="text-sm font-medium text-gray-800">{t('fork.stt.model')}</div>
        <Select value={selectedModel} onValueChange={chooseModel}>
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              <SelectLabel>{t('fork.stt.recommended')}</SelectLabel>
              {RECOMMENDED_STT_MODELS.map(m => (
                <SelectItem key={m.id} value={m.id}>
                  {m.id} — {t(`fork.stt.notes.${m.noteKey}`)} (~{m.costPerHour} $/h)
                </SelectItem>
              ))}
            </SelectGroup>
            {otherModels.length > 0 && (
              <SelectGroup>
                <SelectLabel>{t('fork.stt.otherModels')}</SelectLabel>
                {otherModels.map(m => (
                  <SelectItem key={m.id} value={m.id}>
                    {m.id}
                  </SelectItem>
                ))}
              </SelectGroup>
            )}
          </SelectContent>
        </Select>
        {!isActive && (
          <Button size="sm" onClick={() => chooseModel(selectedModel)}>
            {t('fork.stt.activate')}
          </Button>
        )}
      </div>

      {loaded && (
        <div className="space-y-4">
          <SettingRow label={t('fork.stt.language')} hint={t('fork.stt.languageHint')}>
            <Select value={get(PREF.sttLanguage, 'fr')} onValueChange={value => update({ [PREF.sttLanguage]: value })}>
              <SelectTrigger className="w-44">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="fr">{t('fork.languages.fr')}</SelectItem>
                <SelectItem value="en">{t('fork.languages.en')}</SelectItem>
                <SelectItem value="auto">{t('fork.languages.auto')}</SelectItem>
              </SelectContent>
            </Select>
          </SettingRow>
          <SettingRow label={t('fork.stt.fallback')} hint={t('fork.stt.fallbackHint')}>
            <Switch
              checked={getBool(PREF.sttFallbackLocal)}
              onCheckedChange={checked => update({ [PREF.sttFallbackLocal]: String(checked) })}
            />
          </SettingRow>
          <SettingRow label={t('fork.stt.concurrency')} hint={t('fork.stt.concurrencyHint')}>
            <Select value={get(PREF.sttConcurrency, '3')} onValueChange={value => update({ [PREF.sttConcurrency]: value })}>
              <SelectTrigger className="w-24">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {['1', '2', '3', '4', '6'].map(n => (
                  <SelectItem key={n} value={n}>{n}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </SettingRow>
        </div>
      )}
    </div>
  );
}

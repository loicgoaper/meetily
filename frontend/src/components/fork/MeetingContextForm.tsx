'use client';

import { useTranslation } from 'react-i18next';
import { Monitor, Users, Video } from 'lucide-react';
import { Textarea } from '@/components/ui/textarea';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import type { MeetingContext, MeetingType } from '@/services/forkService';
import { ParticipantsEditor } from './ParticipantsEditor';

/**
 * Context of a meeting: format, participants, background, glossary and (optionally)
 * instructions for the summary and the live screen-capture switch.
 */
export function MeetingContextForm({
  value,
  onChange,
  showInstructions = true,
  showScreenCapture = false,
}: {
  value: MeetingContext;
  onChange: (value: MeetingContext) => void;
  showInstructions?: boolean;
  showScreenCapture?: boolean;
}) {
  const { t } = useTranslation();
  const set = (patch: Partial<MeetingContext>) => onChange({ ...value, ...patch });
  const type: MeetingType = value.meetingType ?? 'visio';

  return (
    <div className="space-y-5">
      <div className="space-y-2">
        <div className="text-sm font-medium text-gray-800">{t('fork.context.format')}</div>
        <div className="grid grid-cols-2 gap-2">
          {([
            { key: 'visio', icon: Video, label: t('fork.context.visio'), hint: t('fork.context.visioHint') },
            { key: 'presentiel', icon: Users, label: t('fork.context.inPerson'), hint: t('fork.context.inPersonHint') },
          ] as const).map(option => (
            <button
              key={option.key}
              type="button"
              onClick={() => set({ meetingType: option.key })}
              className={`flex items-start gap-2 rounded-md border p-3 text-left transition-colors ${
                type === option.key ? 'border-blue-500 bg-blue-50' : 'border-gray-200 hover:bg-gray-50'
              }`}
            >
              <option.icon className="w-4 h-4 mt-0.5 text-gray-600" />
              <span>
                <span className="block text-sm font-medium text-gray-900">{option.label}</span>
                <span className="block text-xs text-gray-500">{option.hint}</span>
              </span>
            </button>
          ))}
        </div>
      </div>

      <div className="space-y-2">
        <div className="text-sm font-medium text-gray-800">{t('fork.context.participants')}</div>
        <ParticipantsEditor participants={value.participants} onChange={participants => set({ participants })} />
      </div>

      <div className="space-y-2">
        <div className="text-sm font-medium text-gray-800">{t('fork.context.background')}</div>
        <Textarea
          className="min-h-[80px]"
          placeholder={t('fork.context.backgroundPlaceholder')}
          value={value.contextText ?? ''}
          onChange={e => set({ contextText: e.target.value })}
        />
      </div>

      <div className="space-y-2">
        <div className="text-sm font-medium text-gray-800">{t('fork.context.glossary')}</div>
        <Input
          placeholder={t('fork.context.glossaryPlaceholder')}
          value={value.glossary ?? ''}
          onChange={e => set({ glossary: e.target.value })}
        />
      </div>

      {showInstructions && (
        <div className="space-y-2">
          <div className="text-sm font-medium text-gray-800">{t('fork.context.instructions')}</div>
          <Textarea
            className="min-h-[60px]"
            placeholder={t('fork.context.instructionsPlaceholder')}
            value={value.extraInstructions ?? ''}
            onChange={e => set({ extraInstructions: e.target.value })}
          />
        </div>
      )}

      {showScreenCapture && (
        <div className="flex items-center justify-between gap-4 rounded-md border border-gray-200 p-3">
          <div className="flex items-start gap-2">
            <Monitor className="w-4 h-4 mt-0.5 text-gray-600" />
            <div>
              <div className="text-sm font-medium text-gray-900">{t('fork.context.captureScreen')}</div>
              <div className="text-xs text-gray-500">{t('fork.context.captureScreenHint')}</div>
            </div>
          </div>
          <Switch checked={!!value.captureScreen} onCheckedChange={checked => set({ captureScreen: checked })} />
        </div>
      )}
    </div>
  );
}

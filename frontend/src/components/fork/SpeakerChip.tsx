'use client';

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { speakerColor, speakerLabel, type SpeakerMap } from '@/lib/speakers';

/** Coloured speaker label in the transcript; click to rename for the whole meeting. */
export function SpeakerChip({
  speakerKey,
  speakers,
  onRename,
}: {
  speakerKey: string | null | undefined;
  speakers: SpeakerMap;
  onRename?: (speakerKey: string, name: string | null) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const label = speakerLabel(speakerKey, speakers, t);
  if (!speakerKey || !label) return null;

  const chip = (
    <span className={`inline-block rounded px-1.5 py-0.5 text-xs font-medium ${speakerColor(speakerKey)}`}>{label}</span>
  );
  if (!onRename) return <span className="mr-1.5 align-middle">{chip}</span>;

  const save = (value: string | null) => {
    onRename(speakerKey, value);
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={next => {
        setOpen(next);
        if (next) setName(speakers[speakerKey]?.displayName ?? '');
      }}
    >
      <PopoverTrigger asChild>
        <button type="button" className="mr-1.5 align-middle" title={t('fork.speakers.rename')}>
          {chip}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-72 space-y-2" align="start">
        <div className="text-xs text-gray-500">{t('fork.speakers.renameHint', { label })}</div>
        <Input
          autoFocus
          value={name}
          placeholder={t('fork.speakers.namePlaceholder')}
          onChange={e => setName(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && save(name.trim() || null)}
        />
        <div className="flex justify-between">
          <Button variant="ghost" size="sm" onClick={() => save(null)}>
            {t('fork.speakers.resetName')}
          </Button>
          <Button size="sm" onClick={() => save(name.trim() || null)}>
            {t('fork.common.save')}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

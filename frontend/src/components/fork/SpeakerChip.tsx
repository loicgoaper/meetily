'use client';

import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, Pause, Play } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { speakerColor, speakerLabel, type SpeakerMap } from '@/lib/speakers';
import { useClipPlayer } from '@/hooks/fork/useClipPlayer';

export interface SpeakerSegment {
  id: string;
  start: number;
  end?: number;
}

/** ▶ button that replays one transcript segment. */
export function SegmentPlayButton({ meetingId, segment }: { meetingId: string; segment: SpeakerSegment }) {
  const { t } = useTranslation();
  const { play, playingKey, loadingKey } = useClipPlayer();
  const key = `${meetingId}:${segment.id}`;
  const playing = playingKey === key;
  return (
    <button
      type="button"
      title={playing ? t('fork.speakers.stop') : t('fork.speakers.play')}
      onClick={() => play(meetingId, segment.start, segment.end ?? segment.start + 8, key)}
      className={`mr-1 inline-flex h-5 w-5 items-center justify-center rounded-full align-middle transition-colors ${
        playing ? 'bg-blue-600 text-white' : 'text-gray-400 hover:bg-gray-200 hover:text-gray-700'
      }`}
    >
      {loadingKey === key ? <Loader2 className="h-3 w-3 animate-spin" /> : playing ? <Pause className="h-3 w-3" /> : <Play className="h-3 w-3" />}
    </button>
  );
}

/**
 * Coloured speaker label in the transcript. In a meeting page it opens a menu to
 * rename the speaker, or tell that it is another speaker (whole speaker or this line).
 */
export function SpeakerChip({
  speakerKey,
  speakers,
  onRename,
  segment,
  speakerKeys,
  onMerge,
  onReassign,
}: {
  speakerKey: string | null | undefined;
  speakers: SpeakerMap;
  onRename?: (speakerKey: string, name: string | null) => void;
  /** The line this chip belongs to (enables "this line only") */
  segment?: SpeakerSegment;
  /** Every speaker key of the meeting (targets of a merge) */
  speakerKeys?: string[];
  /** All lines of `fromKey` become `intoKey` */
  onMerge?: (fromKey: string, intoKey: string) => void;
  /** Only this line becomes `intoKey` */
  onReassign?: (segmentId: string, intoKey: string) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const [target, setTarget] = useState<string>('');
  const [scope, setScope] = useState<'all' | 'line'>('all');
  const label = speakerLabel(speakerKey, speakers, t);
  const others = useMemo(
    () => (speakerKeys ?? []).filter(k => k !== speakerKey && k !== 'mixed'),
    [speakerKeys, speakerKey],
  );
  if (!speakerKey || !label) return null;

  const chip = (
    <span className={`inline-block rounded px-1.5 py-0.5 text-xs font-medium ${speakerColor(speakerKey)}`}>{label}</span>
  );
  if (!onRename) return <span className="mr-1.5 align-middle">{chip}</span>;

  const save = (value: string | null) => {
    onRename(speakerKey, value);
    setOpen(false);
  };

  const applyMerge = () => {
    if (!target) return;
    if (scope === 'line' && segment && onReassign) onReassign(segment.id, target);
    else onMerge?.(speakerKey, target);
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={next => {
        setOpen(next);
        if (next) {
          setName(speakers[speakerKey]?.displayName ?? '');
          setTarget('');
          setScope('all');
        }
      }}
    >
      <PopoverTrigger asChild>
        <button type="button" className="mr-1.5 align-middle" title={t('fork.speakers.rename')}>
          {chip}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-80 space-y-3" align="start">
        <div className="space-y-2">
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
        </div>

        {others.length > 0 && (onMerge || onReassign) && (
          <div className="space-y-2 border-t border-gray-100 pt-3">
            <div className="text-xs font-medium text-gray-700">{t('fork.speakers.sameAs', { label })}</div>
            <Select value={target} onValueChange={setTarget}>
              <SelectTrigger>
                <SelectValue placeholder={t('fork.speakers.chooseSpeaker')} />
              </SelectTrigger>
              <SelectContent>
                {others.map(key => (
                  <SelectItem key={key} value={key}>
                    {speakerLabel(key, speakers, t)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {segment && onReassign && (
              <div className="flex flex-col gap-1 text-sm text-gray-700">
                <label className="flex items-center gap-2">
                  <input type="radio" checked={scope === 'all'} onChange={() => setScope('all')} />
                  {t('fork.speakers.scopeAll', { label })}
                </label>
                <label className="flex items-center gap-2">
                  <input type="radio" checked={scope === 'line'} onChange={() => setScope('line')} />
                  {t('fork.speakers.scopeLine')}
                </label>
              </div>
            )}
            <div className="flex justify-end">
              <Button size="sm" variant="outline" disabled={!target} onClick={applyMerge}>
                {t('fork.speakers.apply')}
              </Button>
            </div>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}

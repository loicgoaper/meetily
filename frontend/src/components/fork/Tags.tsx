'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Check, Plus, Tag as TagIcon } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  createTag,
  getMeetingTags,
  notifyTagsChanged,
  setMeetingTags,
  TAG_COLOR_CLASSES,
  TAG_COLORS,
  type Tag,
} from '@/services/forkService';
import { useTags } from '@/hooks/fork/useTags';

export function TagChip({ tag, small }: { tag: Tag; small?: boolean }) {
  const classes = TAG_COLOR_CLASSES[tag.color] ?? TAG_COLOR_CLASSES.default;
  return (
    <span className={`inline-flex items-center rounded border ${small ? 'px-1 text-[10px]' : 'px-1.5 py-0.5 text-xs'} font-medium ${classes.chip}`}>
      {tag.name}
    </span>
  );
}

/** Small coloured dots (sidebar meeting rows). */
export function TagDots({ tags }: { tags: Tag[] }) {
  if (tags.length === 0) return null;
  return (
    <span className="ml-1 inline-flex shrink-0 items-center gap-0.5" title={tags.map(t => t.name).join(', ')}>
      {tags.slice(0, 4).map(tag => (
        <span key={tag.id} className={`h-2 w-2 rounded-full ${(TAG_COLOR_CLASSES[tag.color] ?? TAG_COLOR_CLASSES.default).dot}`} />
      ))}
    </span>
  );
}

export function ColorPicker({ value, onChange }: { value: string; onChange: (color: string) => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap gap-1">
      {TAG_COLORS.map(color => (
        <button
          key={color}
          type="button"
          title={t(`fork.tags.colors.${color}`)}
          onClick={() => onChange(color)}
          className={`h-5 w-5 rounded-full border-2 ${TAG_COLOR_CLASSES[color].dot} ${value === color ? 'border-gray-900' : 'border-white'}`}
        />
      ))}
    </div>
  );
}

/** Tags of a meeting: check existing tags or create one on the fly. */
export function MeetingTagPicker({ meetingId }: { meetingId: string }) {
  const { t } = useTranslation();
  const { tags, reload } = useTags();
  const [selected, setSelected] = useState<string[]>([]);
  const [newName, setNewName] = useState('');
  const [newColor, setNewColor] = useState<string>('blue');

  useEffect(() => {
    getMeetingTags(meetingId)
      .then(list => setSelected(list.map(t => t.id)))
      .catch(() => setSelected([]));
  }, [meetingId]);

  const save = async (ids: string[]) => {
    setSelected(ids);
    try {
      await setMeetingTags(meetingId, ids);
      notifyTagsChanged();
    } catch (error) {
      toast.error(String(error));
    }
  };

  const create = async () => {
    if (!newName.trim()) return;
    try {
      const tag = await createTag(newName.trim(), newColor);
      setNewName('');
      await reload();
      await save([...selected, tag.id]);
    } catch (error) {
      toast.error(String(error));
    }
  };

  const current = tags.filter(tag => selected.includes(tag.id));

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" title={t('fork.tags.edit')}>
          <TagIcon />
          {current.length === 0 ? t('fork.tags.add') : (
            <span className="flex flex-wrap gap-1">
              {current.map(tag => <TagChip key={tag.id} tag={tag} small />)}
            </span>
          )}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-72 space-y-3" align="start">
        <div className="max-h-56 space-y-1 overflow-y-auto">
          {tags.length === 0 && <div className="text-xs text-gray-500">{t('fork.tags.none')}</div>}
          {tags.map(tag => {
            const checked = selected.includes(tag.id);
            return (
              <button
                key={tag.id}
                type="button"
                onClick={() => save(checked ? selected.filter(id => id !== tag.id) : [...selected, tag.id])}
                className="flex w-full items-center gap-2 rounded px-1.5 py-1 text-left hover:bg-gray-50"
              >
                <span className={`flex h-4 w-4 items-center justify-center rounded border ${checked ? 'border-blue-600 bg-blue-600 text-white' : 'border-gray-300'}`}>
                  {checked && <Check className="h-3 w-3" />}
                </span>
                <TagChip tag={tag} />
              </button>
            );
          })}
        </div>
        <div className="space-y-2 border-t border-gray-100 pt-3">
          <div className="flex gap-2">
            <Input
              placeholder={t('fork.tags.newPlaceholder')}
              value={newName}
              onChange={e => setNewName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && create()}
            />
            <Button size="icon" variant="outline" onClick={create} disabled={!newName.trim()} title={t('fork.tags.create')}>
              <Plus />
            </Button>
          </div>
          <ColorPicker value={newColor} onChange={setNewColor} />
        </div>
      </PopoverContent>
    </Popover>
  );
}

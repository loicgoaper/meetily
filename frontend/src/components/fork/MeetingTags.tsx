'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Check, Loader2, Plus, Sparkles, Tag as TagIcon, X } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  acceptTagSuggestion,
  createTag,
  dismissTagSuggestion,
  getPreferences,
  getTagSuggestions,
  notifyTagsChanged,
  PREF,
  setMeetingTags,
  suggestTags,
  TAG_COLOR_CLASSES,
  TAGS_CHANGED_EVENT,
  type TagSuggestion,
} from '@/services/forkService';
import { useTags } from '@/hooks/fork/useTags';
import { ColorPicker, TagChip } from './Tags';
import { TagManagerDialog } from './TagManager';

/** Tags of a meeting: check existing tags or create one on the fly. */
export function MeetingTagPicker({ meetingId }: { meetingId: string }) {
  const { t } = useTranslation();
  const { tags, assignments, reload } = useTags();
  const selected = assignments[meetingId] ?? [];
  const [newName, setNewName] = useState('');
  const [newColor, setNewColor] = useState<string>('blue');
  const [pickerOpen, setPickerOpen] = useState(false);
  const [managerOpen, setManagerOpen] = useState(false);

  const save = async (ids: string[]) => {
    try {
      await setMeetingTags(meetingId, ids);
      await reload();
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
      await save([...selected, tag.id]);
    } catch (error) {
      toast.error(String(error));
    }
  };

  const current = tags.filter(tag => selected.includes(tag.id));

  return (
    <>
    <Popover open={pickerOpen} onOpenChange={setPickerOpen}>
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
        <div className="border-t border-gray-100 pt-2 text-right">
          <button
            type="button"
            className="text-xs text-blue-600 hover:underline"
            onClick={() => {
              setPickerOpen(false);
              setManagerOpen(true);
            }}
          >
            {t('fork.tags.manage')}
          </button>
        </div>
      </PopoverContent>
    </Popover>
    <TagManagerDialog open={managerOpen} onOpenChange={setManagerOpen} />
    </>
  );
}

/**
 * Up to 5 tags suggested by the AI for this meeting. Generated after the summary,
 * or the first time the meeting is opened; one click adds a tag.
 */
export function TagSuggestionsBar({ meetingId }: { meetingId: string }) {
  const { t } = useTranslation();
  const [items, setItems] = useState<TagSuggestion[]>([]);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      const current = await getTagSuggestions(meetingId);
      setItems(current.items);
      return current.generated;
    } catch (error) {
      console.warn('Could not load tag suggestions:', error);
      return true;
    }
  }, [meetingId]);

  const run = useCallback(async () => {
    setBusy(true);
    try {
      setItems((await suggestTags(meetingId)).items);
    } catch (error) {
      toast.error(String(error));
    } finally {
      setBusy(false);
    }
  }, [meetingId]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const generated = await reload();
      if (cancelled || generated) return;
      const prefs = await getPreferences().catch(() => ({} as Record<string, string>));
      if (!cancelled && prefs[PREF.tagsAutoSuggest] !== 'false') await run();
    })();
    const onTags = () => void reload();
    window.addEventListener(TAGS_CHANGED_EVENT, onTags);
    const unlisten = listen<{ meetingId: string }>('tag-suggestions-updated', event => {
      if (event.payload.meetingId === meetingId) void reload();
    });
    return () => {
      cancelled = true;
      window.removeEventListener(TAGS_CHANGED_EVENT, onTags);
      unlisten.then(fn => fn());
    };
  }, [meetingId, reload, run]);

  const accept = async (names: string[]) => {
    try {
      for (const name of names) await acceptTagSuggestion(meetingId, name);
      notifyTagsChanged();
    } catch (error) {
      toast.error(String(error));
    }
  };

  const dismiss = async (name: string) => {
    setItems(prev => prev.filter(s => s.name !== name));
    await dismissTagSuggestion(meetingId, name).catch(error => toast.error(String(error)));
  };

  if (busy) {
    return (
      <span className="inline-flex items-center gap-1 text-xs text-gray-500">
        <Loader2 className="h-3.5 w-3.5 animate-spin" /> {t('fork.tags.suggesting')}
      </span>
    );
  }

  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      {items.length > 0 && (
        <span className="inline-flex items-center gap-1 text-xs text-gray-500">
          <Sparkles className="h-3.5 w-3.5 text-purple-500" /> {t('fork.tags.suggestions')}
        </span>
      )}
      {items.map(s => {
        const chip = s.color ? (TAG_COLOR_CLASSES[s.color] ?? TAG_COLOR_CLASSES.default).chip : 'bg-white text-gray-700 border-gray-300';
        return (
          <span key={s.name} className={`inline-flex items-center rounded border border-dashed text-xs ${chip}`}>
            <button type="button" className="px-1.5 py-0.5 hover:underline" title={t('fork.tags.acceptSuggestion')} onClick={() => accept([s.name])}>
              + {s.name}
            </button>
            <button type="button" className="pr-1 opacity-50 hover:opacity-100" title={t('fork.tags.dismissSuggestion')} onClick={() => dismiss(s.name)}>
              <X className="h-3 w-3" />
            </button>
          </span>
        );
      })}
      {items.length > 1 && (
        <Button variant="ghost" size="sm" className="h-6 px-2 text-xs" onClick={() => accept(items.map(s => s.name))}>
          {t('fork.tags.acceptAll')}
        </Button>
      )}
      <Button variant="ghost" size="icon" className="h-7 w-7" title={t('fork.tags.suggestAgain')} onClick={run}>
        <Sparkles className="text-purple-500" />
      </Button>
    </span>
  );
}

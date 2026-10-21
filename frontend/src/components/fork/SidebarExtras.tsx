'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { Loader2, Settings2, Sparkles, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { aiSearch, formatTimestamp, TAG_COLOR_CLASSES, type AiSearchResult, type Tag } from '@/services/forkService';
import { TagManagerDialog } from './TagManager';

const FILTER_KEY = 'fork.tagFilter';

/** Selected tag filter, remembered on this device. */
export function useTagFilter() {
  const [selected, setSelected] = useState<string[]>([]);

  useEffect(() => {
    try {
      const raw = window.localStorage.getItem(FILTER_KEY);
      if (raw) setSelected(JSON.parse(raw));
    } catch {
      // per-device convenience only
    }
  }, []);

  const update = useCallback((next: string[]) => {
    setSelected(next);
    try {
      window.localStorage.setItem(FILTER_KEY, JSON.stringify(next));
    } catch {
      // per-device convenience only
    }
  }, []);

  const toggle = useCallback((id: string) => update(selected.includes(id) ? selected.filter(s => s !== id) : [...selected, id]), [selected, update]);

  return { selected, toggle, clear: () => update([]) };
}

interface TreeItem {
  id: string;
  type: 'folder' | 'file';
  children?: TreeItem[];
}

/** Keep meetings carrying every selected tag (folders are kept, their children filtered). */
export function applyTagFilter<T extends TreeItem>(items: T[], selected: string[], tagsFor: (id: string) => Tag[]): T[] {
  if (selected.length === 0) return items;
  const keep = (id: string) => {
    const ids = tagsFor(id).map(t => t.id);
    return selected.every(s => ids.includes(s));
  };
  return items
    .map(item =>
      item.type === 'folder' && item.children
        ? { ...item, children: item.children.filter(child => child.type === 'folder' || keep(child.id)) }
        : item,
    )
    .filter(item => item.type === 'folder' || keep(item.id));
}

/** Tag chips under the search box (click to filter meetings), and the tag manager. */
export function SidebarTagFilter({ tags, selected, onToggle, onClear }: {
  tags: Tag[];
  selected: string[];
  onToggle: (id: string) => void;
  onClear: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mt-2 flex flex-wrap items-center gap-1">
      {tags.map(tag => {
        const active = selected.includes(tag.id);
        const classes = TAG_COLOR_CLASSES[tag.color] ?? TAG_COLOR_CLASSES.default;
        return (
          <button
            key={tag.id}
            type="button"
            onClick={() => onToggle(tag.id)}
            className={`rounded border px-1.5 py-0.5 text-[11px] font-medium transition-opacity ${classes.chip} ${
              selected.length > 0 && !active ? 'opacity-40' : ''
            } ${active ? 'ring-1 ring-gray-500' : ''}`}
          >
            {tag.name}
          </button>
        );
      })}
      {selected.length > 0 && (
        <button type="button" onClick={onClear} className="text-gray-400 hover:text-gray-700" title={t('fork.tags.clearFilter')}>
          <X className="h-3.5 w-3.5" />
        </button>
      )}
      <TagManagerDialog
        trigger={
          <button
            type="button"
            title={t('fork.tags.manage')}
            className="inline-flex items-center gap-1 rounded px-1 py-0.5 text-[11px] text-gray-500 hover:bg-gray-100 hover:text-gray-800"
          >
            <Settings2 className="h-3.5 w-3.5" />
            {tags.length === 0 && t('fork.tags.manage')}
          </button>
        }
      />
    </div>
  );
}

/** "✨ Recherche IA" button and dialog. */
export function AiSearchButton() {
  const { t } = useTranslation();
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<AiSearchResult[] | null>(null);

  const run = async () => {
    if (!query.trim()) return;
    setBusy(true);
    setResults(null);
    try {
      setResults(await aiSearch(query));
    } catch (error) {
      toast.error(String(error));
    } finally {
      setBusy(false);
    }
  };

  const openMeeting = (id: string) => {
    setOpen(false);
    router.push(`/meeting-details?id=${id}`);
  };

  return (
    <>
      <Button variant="outline" size="sm" className="mt-2 w-full justify-start text-gray-600" onClick={() => setOpen(true)}>
        <Sparkles className="text-purple-500" /> {t('fork.search.button')}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t('fork.search.title')}</DialogTitle>
            <DialogDescription>{t('fork.search.description')}</DialogDescription>
          </DialogHeader>
          <div className="flex gap-2">
            <Input
              autoFocus
              placeholder={t('fork.search.placeholder')}
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && run()}
            />
            <Button onClick={run} disabled={busy || !query.trim()}>
              {busy ? <Loader2 className="animate-spin" /> : <Sparkles />} {t('fork.search.run')}
            </Button>
          </div>
          <div className="max-h-[55vh] space-y-2 overflow-y-auto">
            {busy && <div className="py-6 text-center text-sm text-gray-500">{t('fork.search.searching')}</div>}
            {results && results.length === 0 && <div className="py-6 text-center text-sm text-gray-500">{t('fork.search.noResult')}</div>}
            {results?.map(result => (
              <button
                key={result.meetingId}
                type="button"
                onClick={() => openMeeting(result.meetingId)}
                className="block w-full rounded-md border border-gray-200 p-3 text-left hover:border-blue-300 hover:bg-blue-50"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium text-gray-900">{result.title}</span>
                  <span className="shrink-0 text-xs text-gray-500">{result.date}</span>
                </div>
                <div className="mt-1 text-sm text-gray-700">{result.reason}</div>
                {result.hits.length > 0 && (
                  <div className="mt-1 space-y-0.5">
                    {result.hits.slice(0, 2).map((hit, i) => (
                      <div key={i} className="truncate text-xs text-gray-500">
                        {hit.t != null && <span className="mr-1 text-gray-400">[{formatTimestamp(hit.t)}]</span>}
                        {hit.text}
                      </div>
                    ))}
                  </div>
                )}
              </button>
            ))}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}

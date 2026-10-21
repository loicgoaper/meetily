'use client';

import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Check, ListFilter, RotateCcw, Search, X } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Input } from '@/components/ui/input';
import { TAG_COLOR_CLASSES, type Tag } from '@/services/forkService';
import { useTags } from '@/hooks/fork/useTags';
import { fitInLines, foldText } from '@/lib/tag-filter';
import { TagChip } from './Tags';
import { TagManagerDialog } from './TagManager';

const CHIP_GAP = 4; // gap-1
const MAX_LINES = 2;

function FilterChip({ tag, onRemove }: { tag: Tag; onRemove?: () => void }) {
  const { t } = useTranslation();
  const classes = TAG_COLOR_CLASSES[tag.color] ?? TAG_COLOR_CLASSES.default;
  return (
    <button
      type="button"
      data-chip=""
      onClick={onRemove}
      title={t('fork.tags.removeFromFilter')}
      className={`inline-flex max-w-full items-center gap-0.5 rounded border px-1.5 py-0.5 text-[11px] font-medium ${classes.chip}`}
    >
      <span className="truncate">{tag.name}</span>
      <X className="h-3 w-3 shrink-0 opacity-60" />
    </button>
  );
}

/**
 * Tag filter under the sidebar search: "Tout" when nothing is filtered, otherwise the
 * selected tags on at most two lines ("+N" for the rest). The filter icon opens a
 * searchable list of every tag; the reset icon goes back to "Tout".
 */
export function SidebarTagFilter({ tags, selected, onToggle, onClear }: {
  tags: Tag[];
  selected: string[];
  onToggle: (id: string) => void;
  onClear: () => void;
}) {
  const { t } = useTranslation();
  const { assignments } = useTags();
  const [open, setOpen] = useState(false);
  const [managerOpen, setManagerOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [visibleCount, setVisibleCount] = useState(Number.MAX_SAFE_INTEGER);
  const measureRef = useRef<HTMLDivElement>(null);

  const chosen = useMemo(
    () => selected.map(id => tags.find(tag => tag.id === id)).filter((tag): tag is Tag => !!tag),
    [selected, tags],
  );

  // Measure the chips in an invisible copy of the row to know how many fit on two lines
  useLayoutEffect(() => {
    const row = measureRef.current;
    if (!row) return;
    const compute = () => {
      const widths = Array.from(row.querySelectorAll<HTMLElement>('[data-chip]')).map(el => el.getBoundingClientRect().width);
      const plus = row.querySelector<HTMLElement>('[data-plus]')?.getBoundingClientRect().width ?? 32;
      setVisibleCount(fitInLines(widths, plus, row.clientWidth, CHIP_GAP, MAX_LINES));
    };
    compute();
    const observer = new ResizeObserver(compute);
    observer.observe(row);
    return () => observer.disconnect();
  }, [chosen]);

  const shown = chosen.slice(0, visibleCount);
  const hidden = chosen.length - shown.length;

  const usage = useMemo(() => {
    const counts: Record<string, number> = {};
    Object.values(assignments).forEach(ids => ids.forEach(id => (counts[id] = (counts[id] ?? 0) + 1)));
    return counts;
  }, [assignments]);

  const matches = useMemo(() => {
    const q = foldText(query.trim());
    return q ? tags.filter(tag => foldText(tag.name).includes(q)) : tags;
  }, [tags, query]);

  return (
    <>
      <div className="mt-2 flex items-start gap-1">
        <div className="relative min-w-0 flex-1">
          <div className="flex flex-wrap gap-1">
            {chosen.length === 0 ? (
              <button
                type="button"
                onClick={() => setOpen(true)}
                title={t('fork.tags.allHint')}
                className="rounded border border-gray-200 bg-white px-1.5 py-0.5 text-[11px] font-medium text-gray-600 hover:bg-gray-50"
              >
                {t('fork.tags.all')}
              </button>
            ) : (
              <>
                {shown.map(tag => <FilterChip key={tag.id} tag={tag} onRemove={() => onToggle(tag.id)} />)}
                {hidden > 0 && (
                  <button
                    type="button"
                    onClick={() => setOpen(true)}
                    title={t('fork.tags.hiddenCount', { count: hidden })}
                    className="rounded border border-gray-300 bg-gray-100 px-1.5 py-0.5 text-[11px] font-medium text-gray-700 hover:bg-gray-200"
                  >
                    +{hidden}
                  </button>
                )}
              </>
            )}
          </div>
          <div ref={measureRef} aria-hidden className="pointer-events-none invisible absolute inset-x-0 top-0 flex flex-wrap gap-1">
            {chosen.map(tag => <FilterChip key={tag.id} tag={tag} />)}
            <span data-plus="" className="rounded border px-1.5 py-0.5 text-[11px] font-medium">+888</span>
          </div>
        </div>

        <Popover
          open={open}
          onOpenChange={next => {
            setOpen(next);
            if (!next) setQuery('');
          }}
        >
          <PopoverTrigger asChild>
            <button
              type="button"
              title={t('fork.tags.filter')}
              className={`rounded p-1 hover:bg-gray-100 ${chosen.length > 0 ? 'text-blue-600' : 'text-gray-500 hover:text-gray-800'}`}
            >
              <ListFilter className="h-4 w-4" />
            </button>
          </PopoverTrigger>
          <PopoverContent side="right" align="start" className="w-72 space-y-2 p-3">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-gray-400" />
              <Input
                autoFocus
                value={query}
                onChange={e => setQuery(e.target.value)}
                placeholder={t('fork.tags.searchPlaceholder')}
                className="h-8 pl-7 text-sm"
              />
            </div>
            <div className="max-h-72 space-y-0.5 overflow-y-auto">
              {tags.length === 0 && <div className="py-2 text-xs text-gray-500">{t('fork.tags.none')}</div>}
              {tags.length > 0 && matches.length === 0 && <div className="py-2 text-xs text-gray-500">{t('fork.tags.noMatch')}</div>}
              {matches.map(tag => {
                const checked = selected.includes(tag.id);
                return (
                  <button
                    key={tag.id}
                    type="button"
                    onClick={() => onToggle(tag.id)}
                    className="flex w-full items-center gap-2 rounded px-1.5 py-1 text-left hover:bg-gray-50"
                  >
                    <span className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${checked ? 'border-blue-600 bg-blue-600 text-white' : 'border-gray-300'}`}>
                      {checked && <Check className="h-3 w-3" />}
                    </span>
                    <span className="min-w-0 flex-1 truncate">
                      <TagChip tag={tag} />
                    </span>
                    <span className="text-[11px] text-gray-400" title={t('fork.tags.usage', { count: usage[tag.id] ?? 0 })}>
                      {usage[tag.id] ?? 0}
                    </span>
                  </button>
                );
              })}
            </div>
            <div className="flex items-center justify-between border-t border-gray-100 pt-2">
              <button type="button" disabled={chosen.length === 0} onClick={onClear} className="text-xs text-gray-600 hover:underline disabled:opacity-40 disabled:no-underline">
                {t('fork.tags.showAll')}
              </button>
              <button
                type="button"
                onClick={() => {
                  setOpen(false);
                  setManagerOpen(true);
                }}
                className="text-xs text-blue-600 hover:underline"
              >
                {t('fork.tags.manage')}
              </button>
            </div>
          </PopoverContent>
        </Popover>

        {chosen.length > 0 && (
          <button type="button" onClick={onClear} title={t('fork.tags.resetFilter')} className="rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-700">
            <RotateCcw className="h-4 w-4" />
          </button>
        )}
      </div>
      <TagManagerDialog open={managerOpen} onOpenChange={setManagerOpen} />
    </>
  );
}

'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Copy, Download, FilePlus2, Pencil, RotateCcw, Star, Trash2, Upload } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { usePreferences } from '@/hooks/fork/usePreferences';
import {
  deleteCustomTemplate,
  duplicateTemplate,
  exportTemplate,
  getTemplateFull,
  importTemplate,
  listTemplatesFull,
  PREF,
  type Template,
  type TemplateEntry,
} from '@/services/forkService';
import { SettingsCard } from './SettingsCard';
import { emptyTemplate, TemplateEditorDialog } from './TemplateEditorDialog';

/** "Templates" settings tab: list, default, create, edit, duplicate, import/export, delete. */
export function TemplateManager() {
  const { t } = useTranslation();
  const { get, update, loaded } = usePreferences();
  const [entries, setEntries] = useState<TemplateEntry[]>([]);
  const [editor, setEditor] = useState<{ id: string | null; template: Template } | null>(null);
  const [duplicating, setDuplicating] = useState<{ id: string; name: string } | null>(null);
  const defaultId = get(PREF.summaryDefaultTemplate, 'fr_reunion_standard');

  const refresh = useCallback(async () => {
    try {
      const list = await listTemplatesFull();
      // French templates first, then alphabetical
      list.sort((a, b) => Number(b.id.startsWith('fr_')) - Number(a.id.startsWith('fr_')) || a.name.localeCompare(b.name));
      setEntries(list);
    } catch (error) {
      console.error('Failed to list templates:', error);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const openEditor = async (id: string | null) => {
    if (!id) {
      setEditor({ id: null, template: emptyTemplate() });
      return;
    }
    try {
      const full = await getTemplateFull(id);
      setEditor({ id, template: full.template });
    } catch (error) {
      toast.error(String(error));
    }
  };

  const remove = async (entry: TemplateEntry) => {
    try {
      const stillExists = await deleteCustomTemplate(entry.id);
      toast.success(stillExists ? t('fork.templates.restored') : t('fork.templates.deleted'));
      if (!stillExists && defaultId === entry.id) {
        await update({ [PREF.summaryDefaultTemplate]: 'fr_reunion_standard' });
      }
      await refresh();
    } catch (error) {
      toast.error(String(error));
    }
  };

  const doImport = async () => {
    try {
      const id = await importTemplate();
      if (id) {
        toast.success(t('fork.templates.imported'));
        await refresh();
      }
    } catch (error) {
      toast.error(String(error));
    }
  };

  const doExport = async (id: string) => {
    try {
      if (await exportTemplate(id)) toast.success(t('fork.templates.exported'));
    } catch (error) {
      toast.error(String(error));
    }
  };

  const confirmDuplicate = async () => {
    if (!duplicating) return;
    try {
      await duplicateTemplate(duplicating.id, duplicating.name);
      toast.success(t('fork.templates.duplicated'));
      setDuplicating(null);
      await refresh();
    } catch (error) {
      toast.error(String(error));
    }
  };

  const sourceBadge = (entry: TemplateEntry) => {
    if (entry.source === 'custom') {
      return entry.overridesOriginal ? t('fork.templates.badgeModified') : t('fork.templates.badgeCustom');
    }
    return t('fork.templates.badgeBuiltin');
  };

  return (
    <div className="space-y-6">
      <SettingsCard
        title={t('fork.templates.title')}
        description={t('fork.templates.description')}
        action={
          <div className="flex gap-2">
            <Button variant="outline" size="sm" onClick={doImport}>
              <Upload /> {t('fork.templates.import')}
            </Button>
            <Button size="sm" onClick={() => openEditor(null)}>
              <FilePlus2 /> {t('fork.templates.new')}
            </Button>
          </div>
        }
      >
        {loaded && (
          <div className="flex items-center justify-between gap-4 rounded-md bg-blue-50 px-3 py-2">
            <span className="text-sm text-blue-900">{t('fork.templates.default')}</span>
            <Select value={defaultId} onValueChange={v => update({ [PREF.summaryDefaultTemplate]: v })}>
              <SelectTrigger className="w-80 bg-white">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {entries.map(e => (
                  <SelectItem key={e.id} value={e.id}>{e.name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}

        <div className="divide-y divide-gray-100 rounded-md border border-gray-200">
          {entries.map(entry => (
            <div key={entry.id} className="flex items-center gap-3 px-3 py-2.5">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-gray-900 truncate">{entry.name}</span>
                  {entry.id === defaultId && <Star className="w-3.5 h-3.5 text-amber-500 fill-amber-500" />}
                  <span className="rounded bg-gray-100 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-gray-600">
                    {sourceBadge(entry)}
                  </span>
                </div>
                <div className="text-xs text-gray-500 truncate">
                  {entry.description} · {t('fork.templates.sections', { count: entry.sections })}
                </div>
              </div>
              <Button variant="ghost" size="icon" title={t('fork.templates.edit')} onClick={() => openEditor(entry.id)}>
                <Pencil />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                title={t('fork.templates.duplicate')}
                onClick={() => setDuplicating({ id: entry.id, name: t('fork.templates.copyName', { name: entry.name }) })}
              >
                <Copy />
              </Button>
              <Button variant="ghost" size="icon" title={t('fork.templates.export')} onClick={() => doExport(entry.id)}>
                <Download />
              </Button>
              {entry.source === 'custom' && (
                <Button
                  variant="ghost"
                  size="icon"
                  title={entry.overridesOriginal ? t('fork.templates.restore') : t('fork.templates.delete')}
                  onClick={() => remove(entry)}
                >
                  {entry.overridesOriginal ? <RotateCcw /> : <Trash2 className="text-red-600" />}
                </Button>
              )}
            </div>
          ))}
        </div>
      </SettingsCard>

      {editor && (
        <TemplateEditorDialog
          open={!!editor}
          onOpenChange={open => !open && setEditor(null)}
          templateId={editor.id}
          initial={editor.template}
          onSaved={() => void refresh()}
        />
      )}

      <Dialog open={!!duplicating} onOpenChange={open => !open && setDuplicating(null)}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t('fork.templates.duplicateTitle')}</DialogTitle>
          </DialogHeader>
          <Input
            autoFocus
            value={duplicating?.name ?? ''}
            onChange={e => duplicating && setDuplicating({ ...duplicating, name: e.target.value })}
            onKeyDown={e => e.key === 'Enter' && confirmDuplicate()}
          />
          <DialogFooter>
            <Button variant="ghost" onClick={() => setDuplicating(null)}>{t('fork.common.cancel')}</Button>
            <Button onClick={confirmDuplicate} disabled={!duplicating?.name.trim()}>{t('fork.templates.duplicate')}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

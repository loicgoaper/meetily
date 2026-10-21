'use client';

import { useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { createTag, deleteTag, getPreferences, notifyTagsChanged, PREF, setPreferences, updateTag, type Tag } from '@/services/forkService';
import { useTags } from '@/hooks/fork/useTags';
import { SettingRow, SettingsCard } from './SettingsCard';
import { ColorPicker, TagChip } from './Tags';

function TagRow({ tag, usage }: { tag: Tag; usage: number }) {
  const { t } = useTranslation();
  const [name, setName] = useState(tag.name);
  const [confirming, setConfirming] = useState(false);

  const save = async (nextName: string, color: string) => {
    try {
      await updateTag(tag.id, nextName.trim() || tag.name, color);
      notifyTagsChanged();
    } catch (error) {
      toast.error(String(error));
    }
  };

  const remove = async () => {
    try {
      await deleteTag(tag.id);
      notifyTagsChanged();
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-3 px-3 py-2">
      <span className="w-36 shrink-0 truncate">
        <TagChip tag={tag} />
      </span>
      <Input
        className="h-8 w-48"
        value={name}
        onChange={e => setName(e.target.value)}
        onBlur={() => name !== tag.name && save(name, tag.color)}
        onKeyDown={e => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
      />
      <ColorPicker value={tag.color} onChange={color => save(name, color)} />
      <span className="text-xs text-gray-500">{t('fork.tags.usage', { count: usage })}</span>
      {confirming ? (
        <span className="ml-auto inline-flex items-center gap-2 text-xs text-gray-700">
          {usage > 0 ? t('fork.tags.deleteConfirm', { name: tag.name }) : t('fork.tags.deleteConfirmUnused')}
          <Button size="sm" variant="destructive" className="h-7" onClick={remove}>{t('fork.tags.deleteConfirmButton')}</Button>
          <Button size="sm" variant="ghost" className="h-7" onClick={() => setConfirming(false)}>{t('fork.common.cancel')}</Button>
        </span>
      ) : (
        <Button className="ml-auto" variant="ghost" size="icon" title={t('fork.tags.delete')} onClick={() => setConfirming(true)}>
          <Trash2 className="text-red-600" />
        </Button>
      )}
    </div>
  );
}

/** Create a tag in advance, then rename, recolour or delete every tag. */
function TagManagerBody() {
  const { t } = useTranslation();
  const { tags, assignments } = useTags();
  const [name, setName] = useState('');
  const [color, setColor] = useState('blue');
  const usage = (id: string) => Object.values(assignments).filter(ids => ids.includes(id)).length;

  const create = async () => {
    if (!name.trim()) return;
    try {
      await createTag(name, color);
      setName('');
      notifyTagsChanged();
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <Input className="w-56" placeholder={t('fork.tags.newPlaceholder')} value={name} onChange={e => setName(e.target.value)} onKeyDown={e => e.key === 'Enter' && create()} />
        <ColorPicker value={color} onChange={setColor} />
        <Button size="sm" onClick={create} disabled={!name.trim()}>
          <Plus /> {t('fork.tags.create')}
        </Button>
      </div>
      {tags.length === 0 ? (
        <p className="text-sm text-gray-500">{t('fork.tags.emptyList')}</p>
      ) : (
        <div className="max-h-[50vh] divide-y divide-gray-100 overflow-y-auto rounded-md border border-gray-200">
          {tags.map(tag => <TagRow key={`${tag.id}-${tag.name}-${tag.color}`} tag={tag} usage={usage(tag.id)} />)}
        </div>
      )}
    </div>
  );
}

function AutoSuggestRow() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(true);

  useEffect(() => {
    getPreferences()
      .then(prefs => setEnabled(prefs[PREF.tagsAutoSuggest] !== 'false'))
      .catch(() => undefined);
  }, []);

  return (
    <SettingRow label={t('fork.tags.autoSuggest')} hint={t('fork.tags.autoSuggestHint')}>
      <Switch
        checked={enabled}
        onCheckedChange={async checked => {
          setEnabled(checked);
          await setPreferences({ [PREF.tagsAutoSuggest]: String(checked) }).catch(error => toast.error(String(error)));
        }}
      />
    </SettingRow>
  );
}

/** Settings → Tags. */
export function TagManager() {
  const { t } = useTranslation();
  return (
    <div className="space-y-6">
      <SettingsCard title={t('fork.tags.title')} description={t('fork.tags.description')}>
        <TagManagerBody />
      </SettingsCard>
      <SettingsCard title={t('fork.tags.suggestionsTitle')}>
        <AutoSuggestRow />
      </SettingsCard>
    </div>
  );
}

/** The same manager in a dialog (sidebar and meeting tag picker). */
export function TagManagerDialog({ trigger, open, onOpenChange }: {
  trigger?: ReactNode;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {trigger && <DialogTrigger asChild>{trigger}</DialogTrigger>}
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t('fork.tags.title')}</DialogTitle>
          <DialogDescription>{t('fork.tags.description')}</DialogDescription>
        </DialogHeader>
        <TagManagerBody />
      </DialogContent>
    </Dialog>
  );
}

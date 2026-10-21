'use client';

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { createTag, deleteTag, notifyTagsChanged, updateTag, type Tag } from '@/services/forkService';
import { useTags } from '@/hooks/fork/useTags';
import { SettingsCard } from './SettingsCard';
import { ColorPicker, TagChip } from './Tags';

function TagRow({ tag, usage }: { tag: Tag; usage: number }) {
  const { t } = useTranslation();
  const [name, setName] = useState(tag.name);

  const save = async (nextName: string, color: string) => {
    try {
      await updateTag(tag.id, nextName.trim() || tag.name, color);
      notifyTagsChanged();
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-3 px-3 py-2">
      <TagChip tag={tag} />
      <Input className="h-8 w-48" value={name} onChange={e => setName(e.target.value)} onBlur={() => name !== tag.name && save(name, tag.color)} />
      <ColorPicker value={tag.color} onChange={color => save(name, color)} />
      <span className="text-xs text-gray-500">{t('fork.tags.usage', { count: usage })}</span>
      <Button
        className="ml-auto"
        variant="ghost"
        size="icon"
        title={t('fork.tags.delete')}
        onClick={async () => {
          if (!window.confirm(t('fork.tags.deleteConfirm', { name: tag.name }))) return;
          try {
            await deleteTag(tag.id);
            notifyTagsChanged();
          } catch (error) {
            toast.error(String(error));
          }
        }}
      >
        <Trash2 className="text-red-600" />
      </Button>
    </div>
  );
}

/** Settings card: create, rename, recolour and delete tags. */
export function TagManager() {
  const { t } = useTranslation();
  const { tags, assignments } = useTags();
  const [name, setName] = useState('');
  const [color, setColor] = useState('blue');
  const usage = (id: string) => Object.values(assignments).filter(ids => ids.includes(id)).length;

  const create = async () => {
    try {
      await createTag(name, color);
      setName('');
      notifyTagsChanged();
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <SettingsCard title={t('fork.tags.title')} description={t('fork.tags.description')}>
      <div className="flex flex-wrap items-center gap-3">
        <Input className="w-56" placeholder={t('fork.tags.newPlaceholder')} value={name} onChange={e => setName(e.target.value)} onKeyDown={e => e.key === 'Enter' && name.trim() && create()} />
        <ColorPicker value={color} onChange={setColor} />
        <Button size="sm" onClick={create} disabled={!name.trim()}>
          <Plus /> {t('fork.tags.create')}
        </Button>
      </div>
      {tags.length > 0 && (
        <div className="divide-y divide-gray-100 rounded-md border border-gray-200">
          {tags.map(tag => <TagRow key={`${tag.id}-${tag.color}`} tag={tag} usage={usage(tag.id)} />)}
        </div>
      )}
    </SettingsCard>
  );
}

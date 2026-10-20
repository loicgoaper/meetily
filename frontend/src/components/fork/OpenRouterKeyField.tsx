'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { KeyRound } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { getOpenRouterKeyStatus, setOpenRouterKey, type KeyStatus } from '@/services/forkService';

/**
 * The single OpenRouter key of the app (summary, transcription, speaker naming,
 * vision). Shown masked; replacing it changes it everywhere.
 */
export function OpenRouterKeyField({ onChange }: { onChange?: (status: KeyStatus) => void }) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<KeyStatus | null>(null);
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState('');
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    getOpenRouterKeyStatus()
      .then(s => {
        setStatus(s);
        onChange?.(s);
      })
      .catch(error => console.error('Failed to read OpenRouter key status:', error));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = async () => {
    setSaving(true);
    try {
      const next = await setOpenRouterKey(value);
      setStatus(next);
      onChange?.(next);
      setEditing(false);
      setValue('');
      toast.success(t('fork.key.saved'));
    } catch (error) {
      toast.error(String(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2 text-sm font-medium text-gray-800">
        <KeyRound className="w-4 h-4 text-gray-500" />
        {t('fork.key.label')}
      </div>
      {!editing ? (
        <div className="flex items-center justify-between gap-3 rounded-md border border-gray-200 bg-gray-50 px-3 py-2">
          <span className="font-mono text-xs text-gray-700 truncate">
            {status?.configured ? status.masked : t('fork.key.missing')}
          </span>
          <Button variant="outline" size="sm" onClick={() => setEditing(true)}>
            {status?.configured ? t('fork.key.change') : t('fork.key.add')}
          </Button>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <Input
            type="password"
            autoFocus
            placeholder="sk-or-v1-…"
            value={value}
            onChange={e => setValue(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && value.trim() && save()}
          />
          <Button size="sm" onClick={save} disabled={!value.trim() || saving}>
            {t('fork.common.save')}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => { setEditing(false); setValue(''); }}>
            {t('fork.common.cancel')}
          </Button>
        </div>
      )}
      <p className="text-xs text-gray-500">{t('fork.key.hint')}</p>
    </div>
  );
}

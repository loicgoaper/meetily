'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { ExternalLink, KeyRound, Loader2, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import {
  createNotionDatabase,
  getNotionStatus,
  listNotionPages,
  setNotionToken,
  setPreferences,
  syncAllToNotion,
  type NotionPage,
  type NotionStatus,
} from '@/services/forkService';
import { SettingRow, SettingsCard } from './SettingsCard';
import { openExternal } from './NotionMeetingBadge';

/** Settings → Notion: key, parent page, database, automatic sync, sync everything. */
export function NotionSettings() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<NotionStatus | null>(null);
  const [pages, setPages] = useState<NotionPage[] | null>(null);
  const [parent, setParent] = useState('');
  const [editingKey, setEditingKey] = useState(false);
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number; failed: number } | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await getNotionStatus();
      setStatus(s);
      if (s.parentPageId) setParent(s.parentPageId);
    } catch (error) {
      toast.error(String(error));
    }
  }, []);

  const loadPages = useCallback(async () => {
    try {
      setPages(await listNotionPages());
    } catch (error) {
      toast.error(String(error));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const unlisten = listen<{ done: number; total: number; failed: number }>('notion-sync-progress', event => setProgress(event.payload));
    return () => {
      unlisten.then(fn => fn());
    };
  }, [refresh]);

  useEffect(() => {
    if (status?.hasToken && pages === null) void loadPages();
  }, [status?.hasToken, pages, loadPages]);

  const saveKey = async () => {
    setBusy(true);
    try {
      await setNotionToken(key);
      setEditingKey(false);
      setKey('');
      setPages(null);
      await refresh();
      toast.success(t('fork.notion.keySaved'));
    } catch (error) {
      toast.error(String(error));
    } finally {
      setBusy(false);
    }
  };

  const createDatabase = async () => {
    if (!parent) return;
    setBusy(true);
    try {
      await createNotionDatabase(parent);
      await refresh();
      toast.success(t('fork.notion.databaseCreated'));
    } catch (error) {
      toast.error(String(error));
    } finally {
      setBusy(false);
    }
  };

  const syncAll = async () => {
    try {
      const total = await syncAllToNotion();
      setProgress({ done: 0, total, failed: 0 });
    } catch (error) {
      toast.error(String(error));
    }
  };

  const syncing = progress !== null && progress.done < progress.total;

  return (
    <div className="space-y-6">
      <SettingsCard title={t('fork.notion.title')} description={t('fork.notion.description')}>
        <div className="space-y-2">
          <div className="flex items-center gap-2 text-sm font-medium text-gray-800">
            <KeyRound className="h-4 w-4 text-gray-500" />
            {t('fork.notion.key')}
          </div>
          {!editingKey ? (
            <div className="flex items-center justify-between gap-3 rounded-md border border-gray-200 bg-gray-50 px-3 py-2">
              <span className="truncate font-mono text-xs text-gray-700">
                {status?.hasToken ? status.tokenMasked : t('fork.notion.noKey')}
                {status?.integration && <span className="ml-2 font-sans text-gray-500">· {status.integration} · {status.workspace}</span>}
              </span>
              <Button variant="outline" size="sm" onClick={() => setEditingKey(true)}>
                {status?.hasToken ? t('fork.key.change') : t('fork.key.add')}
              </Button>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <Input type="password" autoFocus placeholder="ntn_… / secret_…" value={key} onChange={e => setKey(e.target.value)} />
              <Button size="sm" onClick={saveKey} disabled={busy || !key.trim()}>{t('fork.common.save')}</Button>
              <Button size="sm" variant="ghost" onClick={() => setEditingKey(false)}>{t('fork.common.cancel')}</Button>
            </div>
          )}
          {status?.error && <p className="text-xs text-red-600">{status.error}</p>}
          <div className="flex items-start justify-between gap-3">
            <p className="text-xs text-gray-500">{t('fork.notion.keyHint')}</p>
            {!status?.hasToken && (
              <Button variant="outline" size="sm" className="shrink-0" onClick={() => openExternal('https://www.notion.so/profile/integrations')}>
                <ExternalLink /> {t('fork.notion.openIntegrations')}
              </Button>
            )}
          </div>
        </div>

        {status?.hasToken && (
          <div className="space-y-2">
            <div className="text-sm font-medium text-gray-800">{t('fork.notion.database')}</div>
            {status.databaseId ? (
              <div className="flex items-center justify-between gap-3 rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm text-green-900">
                <span>{t('fork.notion.databaseReady')}</span>
                {status.databaseUrl && (
                  <Button variant="outline" size="sm" onClick={() => openExternal(status.databaseUrl!)}>
                    <ExternalLink /> {t('fork.notion.open')}
                  </Button>
                )}
              </div>
            ) : (
              <p className="text-xs text-gray-600">{t('fork.notion.shareHint', { integration: status.integration ?? 'Meetily' })}</p>
            )}
            <div className="flex items-center gap-2">
              <Select value={parent} onValueChange={setParent}>
                <SelectTrigger className="flex-1">
                  <SelectValue placeholder={pages && pages.length === 0 ? t('fork.notion.noPage') : t('fork.notion.choosePage')} />
                </SelectTrigger>
                <SelectContent>
                  {(pages ?? []).map(p => (
                    <SelectItem key={p.id} value={p.id}>{p.title}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button variant="ghost" size="icon" onClick={loadPages} title={t('fork.notion.refreshPages')}>
                <RefreshCw />
              </Button>
              <Button size="sm" onClick={createDatabase} disabled={busy || !parent}>
                {busy ? <Loader2 className="animate-spin" /> : null}
                {status.databaseId ? t('fork.notion.recreate') : t('fork.notion.create')}
              </Button>
            </div>
          </div>
        )}
      </SettingsCard>

      {status?.databaseId && (
        <SettingsCard title={t('fork.notion.syncTitle')} description={t('fork.notion.syncDescription')}>
          <SettingRow label={t('fork.notion.auto')} hint={t('fork.notion.autoHint')}>
            <Switch
              checked={status.autoSync}
              onCheckedChange={async checked => {
                await setPreferences({ 'notion.auto_sync': String(checked) });
                await refresh();
              }}
            />
          </SettingRow>
          <SettingRow
            label={t('fork.notion.syncAll')}
            hint={progress ? t('fork.notion.progress', progress) : t('fork.notion.syncAllHint')}
          >
            <Button size="sm" variant="outline" onClick={syncAll} disabled={syncing}>
              {syncing ? <Loader2 className="animate-spin" /> : <RefreshCw />} {t('fork.notion.syncAll')}
            </Button>
          </SettingRow>
        </SettingsCard>
      )}
    </div>
  );
}

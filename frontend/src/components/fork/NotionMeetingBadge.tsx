'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { AlertTriangle, ExternalLink, Loader2, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { getNotionMeetingState, getNotionStatus, syncMeetingToNotion, type NotionMeetingState } from '@/services/forkService';

/** Open a URL in the default browser (upstream command). */
export async function openExternal(url: string) {
  try {
    await invoke('open_external_url', { url });
  } catch (error) {
    toast.error(String(error));
  }
}

/** Notion state of the meeting: link to its page, pending/error, sync now. */
export function NotionMeetingBadge({ meetingId }: { meetingId: string }) {
  const { t } = useTranslation();
  const [configured, setConfigured] = useState(false);
  const [state, setState] = useState<NotionMeetingState | null>(null);
  const [syncing, setSyncing] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setState(await getNotionMeetingState(meetingId));
    } catch {
      setState(null);
    }
  }, [meetingId]);

  useEffect(() => {
    getNotionStatus()
      .then(s => setConfigured(s.hasToken && !!s.databaseId))
      .catch(() => setConfigured(false));
    void refresh();
    const unlisten = listen<{ meetingId: string }>('notion-sync-status', event => {
      if (event.payload.meetingId === meetingId) void refresh();
    });
    return () => {
      unlisten.then(fn => fn());
    };
  }, [meetingId, refresh]);

  if (!configured) return null;

  const syncNow = async () => {
    setSyncing(true);
    try {
      await syncMeetingToNotion(meetingId);
      toast.success(t('fork.notion.synced'));
      await refresh();
    } catch (error) {
      toast.error(String(error));
      await refresh();
    } finally {
      setSyncing(false);
    }
  };

  return (
    <span className="inline-flex items-center gap-1">
      {state?.pageUrl && (
        <Button variant="ghost" size="sm" onClick={() => openExternal(state.pageUrl!)} title={t('fork.notion.openPage')}>
          <ExternalLink /> Notion
        </Button>
      )}
      {state?.lastError && (
        <span title={state.lastError} className="text-amber-600">
          <AlertTriangle className="h-4 w-4" />
        </span>
      )}
      <Button
        variant="ghost"
        size="icon"
        onClick={syncNow}
        disabled={syncing}
        title={state?.pending ? t('fork.notion.pending') : t('fork.notion.syncNow')}
      >
        {syncing ? <Loader2 className="animate-spin" /> : <RefreshCw className={state?.pending ? 'text-blue-600' : ''} />}
      </Button>
    </span>
  );
}

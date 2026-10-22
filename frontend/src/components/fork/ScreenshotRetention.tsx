'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Loader2, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { previewRetention, runRetentionNow, type RetentionReport } from '@/services/forkService';

const megabytes = (bytes: number) => (bytes / (1024 * 1024)).toFixed(1);

/** What the retention would delete now, and a button to do it. */
export function ScreenshotRetention({ days }: { days: string }) {
  const { t } = useTranslation();
  const [report, setReport] = useState<RetentionReport | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    previewRetention().then(setReport).catch(() => setReport(null));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh, days]);

  if (!report || days === '0') return null;

  const clean = async () => {
    if (!window.confirm(t('fork.retention.confirm', { count: report.files }))) return;
    setBusy(true);
    try {
      const done = await runRetentionNow();
      toast.success(t('fork.retention.done', { count: done.files, size: megabytes(done.bytes) }));
      refresh();
    } catch (error) {
      toast.error(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex items-center justify-between gap-3 rounded-md bg-gray-50 px-3 py-2 text-xs text-gray-600">
      <span>
        {report.files === 0
          ? t('fork.retention.nothing')
          : t('fork.retention.pending', { count: report.files, size: megabytes(report.bytes), meetings: report.meetings.length })}
      </span>
      {report.files > 0 && (
        <Button size="sm" variant="outline" onClick={clean} disabled={busy}>
          {busy ? <Loader2 className="animate-spin" /> : <Trash2 />} {t('fork.retention.cleanNow')}
        </Button>
      )}
    </div>
  );
}

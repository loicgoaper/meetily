'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, Monitor, RotateCcw, ScanEye, Users } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { formatUsd, getMeetingUsage, hasLiveTranscriptBackup, listMeetingScreenshots } from '@/services/forkService';
import type { usePostMeetingPipeline } from '@/hooks/fork/usePostMeetingPipeline';
import { ScreenshotGallery } from './ScreenshotGallery';
import { MeetingTagPicker } from './Tags';
import { ExportButton } from './ExportDialog';
import { NotionMeetingBadge } from './NotionMeetingBadge';

type Pipeline = ReturnType<typeof usePostMeetingPipeline>;

/**
 * Cloud steps of a meeting: progress of speaker identification / screen reading,
 * manual re-runs, restore of the live transcript, screenshots and cloud cost.
 */
export function MeetingCloudBar({ meetingId, pipeline }: { meetingId: string; pipeline: Pipeline }) {
  const { t } = useTranslation();
  const [hasBackup, setHasBackup] = useState(false);
  const [screenshotCount, setScreenshotCount] = useState(0);
  const [cost, setCost] = useState<number | null>(null);
  const [showScreens, setShowScreens] = useState(false);
  const busy = pipeline.stage === 'diarizing' || pipeline.stage === 'vision';

  useEffect(() => {
    if (busy) return;
    hasLiveTranscriptBackup(meetingId).then(setHasBackup).catch(() => setHasBackup(false));
    listMeetingScreenshots(meetingId).then(f => setScreenshotCount(f.length)).catch(() => setScreenshotCount(0));
    getMeetingUsage(meetingId)
      .then(rows => {
        const total = rows.reduce((sum, r) => sum + (r.costUsd ?? 0), 0);
        setCost(rows.length ? total : null);
      })
      .catch(() => setCost(null));
  }, [meetingId, busy, pipeline.diarization, pipeline.vision]);

  return (
    <div className="border-b border-gray-200 bg-gray-50">
      {busy && (
        <div className="space-y-1 px-4 py-2">
          <div className="flex items-center gap-2 text-sm text-gray-700">
            <Loader2 className="w-4 h-4 animate-spin" />
            {pipeline.stage === 'diarizing'
              ? pipeline.progress?.message || t('fork.pipeline.diarizing')
              : t('fork.pipeline.readingScreens')}
          </div>
          {pipeline.stage === 'diarizing' && <Progress value={pipeline.progress?.value ?? 5} className="h-1" />}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-1 border-b border-gray-100 px-3 py-1.5">
        <MeetingTagPicker meetingId={meetingId} />
        <span className="ml-auto inline-flex items-center gap-1">
          <ExportButton meetingId={meetingId} hasScreenshots={screenshotCount > 0} />
          <NotionMeetingBadge meetingId={meetingId} />
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-1 px-3 py-1.5">
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onClick={() => {
            // A previous run exists: re-running replaces the transcript and manual speaker names
            if (hasBackup && !window.confirm(t('fork.pipeline.rerunConfirm'))) return;
            void pipeline.runDiarization();
          }}
          title={t('fork.pipeline.identifyHint')}
        >
          <Users /> {t('fork.pipeline.identify')}
        </Button>
        {hasBackup && (
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => pipeline.revert()} title={t('fork.pipeline.restoreLiveHint')}>
            <RotateCcw /> {t('fork.pipeline.restoreLive')}
          </Button>
        )}
        {screenshotCount > 0 && (
          <>
            <Button variant={showScreens ? 'secondary' : 'ghost'} size="sm" onClick={() => setShowScreens(s => !s)}>
              <Monitor /> {t('fork.pipeline.screens', { count: screenshotCount })}
            </Button>
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => pipeline.runVision(true)} title={t('fork.pipeline.rereadHint')}>
              <ScanEye /> {t('fork.pipeline.reread')}
            </Button>
          </>
        )}
        {cost !== null && (
          <span className="ml-auto text-xs text-gray-500" title={t('fork.pipeline.costHint')}>
            {t('fork.pipeline.cost', { cost: formatUsd(cost) })}
          </span>
        )}
      </div>

      {showScreens && (
        <div className="max-h-[45vh] overflow-y-auto border-t border-gray-200 bg-white">
          <ScreenshotGallery meetingId={meetingId} refreshKey={pipeline.vision ? 1 : 0} />
        </div>
      )}
    </div>
  );
}

'use client';

import { useEffect, useState, type RefObject } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { ImagePlus, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { illustrateSummary, listMeetingScreenshots } from '@/services/forkService';
import { applyIllustrations, collectIllustrationBlocks, isScreenImage, type AnyBlock } from '@/lib/summary-illustrations';

/**
 * "Illustrer avec les captures": the AI places the relevant screenshots of the meeting
 * next to the points of the summary they support (re-reading the screen first if
 * needed), then the summary is saved.
 */
export function IllustrateSummaryButton({ meetingId, summaryRef, disabled }: {
  meetingId: string;
  summaryRef: RefObject<BlockNoteSummaryViewRef>;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const [available, setAvailable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);

  useEffect(() => {
    listMeetingScreenshots(meetingId)
      .then(frames => setAvailable(frames.some(f => !f.excluded)))
      .catch(() => setAvailable(false));
  }, [meetingId]);

  if (!available) return null;

  const run = async () => {
    const view = summaryRef.current;
    const editor = view?.getEditor();
    if (!view || !editor) {
      toast.error(t('fork.illustrate.noEditor'));
      return;
    }
    const document = editor.document as AnyBlock[];
    const hadImages = document.some(isScreenImage);
    if (hadImages && !window.confirm(t('fork.illustrate.replaceConfirm'))) return;

    setBusy(true);
    const unlisten = await listen<{ meetingId: string; batch?: number; done?: number; total: number }>('vision-progress', event => {
      if (event.payload.meetingId === meetingId) {
        setProgress(t('fork.illustrate.reading', { done: event.payload.batch ?? event.payload.done ?? 0, total: event.payload.total }));
      }
    });
    try {
      const { inputs, ids, kinds } = collectIllustrationBlocks(document);
      const result = await illustrateSummary(meetingId, inputs);
      setProgress(null);
      const count = applyIllustrations(editor as any, ids, kinds, result.illustrations);
      if (count > 0 || hadImages) await view.persist();
      if (count > 0) toast.success(t('fork.illustrate.done', { count }));
      else toast.info(result.candidates === 0 ? t('fork.illustrate.noCandidates') : t('fork.illustrate.none'));
    } catch (error) {
      toast.error(String(error));
    } finally {
      unlisten();
      setBusy(false);
      setProgress(null);
    }
  };

  return (
    <Button variant="outline" size="sm" onClick={run} disabled={busy || disabled} title={t('fork.illustrate.hint')}>
      {busy ? <Loader2 className="animate-spin" /> : <ImagePlus />}
      {progress ?? t('fork.illustrate.button')}
    </Button>
  );
}

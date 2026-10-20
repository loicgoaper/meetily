'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import {
  analyzeScreenshots,
  diarizeMeeting,
  getOpenRouterKeyStatus,
  getPreferences,
  listMeetingScreenshots,
  PREF,
  revertDiarization,
  type DiarizationResult,
  type VisionResult,
} from '@/services/forkService';

export type PipelineStage = 'idle' | 'diarizing' | 'vision' | 'done';

/** Meetings already post-processed in this session (guards remounts). */
const processedMeetings = new Set<string>();
/**
 * Automatic runs in progress, kept outside React: leaving the meeting page and coming
 * back must not let the summary start before speaker identification is over.
 */
const activeRuns = new Map<string, Promise<void>>();

/**
 * After a recording: identify speakers (replaces the live transcript), read the
 * screenshots, then let the summary run. Failures never block the summary.
 */
export function usePostMeetingPipeline({
  meetingId,
  fromRecording,
  onTranscriptsChanged,
}: {
  meetingId: string;
  fromRecording: boolean;
  onTranscriptsChanged?: () => Promise<void> | void;
}) {
  const { t } = useTranslation();
  const runningElsewhere = activeRuns.has(meetingId);
  const shouldStart = fromRecording && !processedMeetings.has(meetingId) && !runningElsewhere;
  const [stage, setStage] = useState<PipelineStage>(runningElsewhere ? 'diarizing' : shouldStart ? 'idle' : 'done');
  // True until the whole automatic sequence is over (the summary waits for it)
  const [autoRunning, setAutoRunning] = useState(shouldStart || runningElsewhere);
  const [progress, setProgress] = useState<{ value: number; message: string } | null>(null);
  const [diarization, setDiarization] = useState<DiarizationResult | null>(null);
  const [vision, setVision] = useState<VisionResult | null>(null);
  const onChangedRef = useRef(onTranscriptsChanged);
  onChangedRef.current = onTranscriptsChanged;
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ meetingId: string; progress: number; message: string }>('diarization-progress', event => {
      if (event.payload.meetingId === meetingId && mountedRef.current) {
        setProgress({ value: event.payload.progress, message: event.payload.message });
      }
    });
    return () => {
      unlisten.then(fn => fn());
    };
  }, [meetingId]);

  const runDiarization = useCallback(async (): Promise<boolean> => {
    setStage('diarizing');
    setProgress({ value: 0, message: t('fork.pipeline.diarizing') });
    try {
      const result = await diarizeMeeting(meetingId);
      if (mountedRef.current) setDiarization(result);
      await onChangedRef.current?.();
      toast.success(t('fork.pipeline.diarized', { count: result.speakers, speakers: result.speakers, named: result.named }));
      return true;
    } catch (error) {
      console.warn('Speaker identification failed:', error);
      toast.warning(t('fork.pipeline.diarizeFailed'), { description: String(error) });
      return false;
    } finally {
      if (mountedRef.current) {
        setProgress(null);
        setStage('done');
      }
    }
  }, [meetingId, t]);

  const runVision = useCallback(async (force = false): Promise<void> => {
    setStage('vision');
    try {
      const result = await analyzeScreenshots(meetingId, force);
      if (mountedRef.current) setVision(result);
      if (result.frames > 0 && !result.cached) {
        toast.success(t('fork.pipeline.visionDone', { count: result.notes }));
      }
    } catch (error) {
      console.warn('Screenshot reading failed:', error);
      toast.warning(t('fork.pipeline.visionFailed'), { description: String(error) });
    } finally {
      if (mountedRef.current) setStage('done');
    }
  }, [meetingId, t]);

  const revert = useCallback(async () => {
    try {
      await revertDiarization(meetingId);
      setDiarization(null);
      await onChangedRef.current?.();
      toast.success(t('fork.pipeline.reverted'));
    } catch (error) {
      toast.error(String(error));
    }
  }, [meetingId, t]);

  useEffect(() => {
    const finish = () => {
      if (!mountedRef.current) return;
      setStage('done');
      setAutoRunning(false);
    };

    // Coming back while the automatic run started by another mount is still going
    const existing = activeRuns.get(meetingId);
    if (existing) {
      existing.finally(async () => {
        if (!mountedRef.current) return;
        await onChangedRef.current?.();
        finish();
      });
      return;
    }
    if (!shouldStart) return;
    processedMeetings.add(meetingId);

    const run = (async () => {
      let hasKey = false;
      let autoDiarize = false;
      try {
        const [key, prefs] = await Promise.all([getOpenRouterKeyStatus(), getPreferences()]);
        hasKey = key.configured;
        autoDiarize = prefs[PREF.diarizationAuto] === 'true';
      } catch (error) {
        console.warn('Post-meeting pipeline: could not read settings:', error);
      }
      if (hasKey && autoDiarize) {
        await runDiarization();
      }
      if (hasKey) {
        try {
          const frames = await listMeetingScreenshots(meetingId);
          if (frames.some(f => !f.excluded)) await runVision();
        } catch (error) {
          console.warn('Post-meeting pipeline: could not list screenshots:', error);
        }
      }
    })();
    activeRuns.set(meetingId, run);
    run.finally(() => {
      activeRuns.delete(meetingId);
      finish();
    });
    // run once per meeting
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId]);

  return {
    stage,
    isDone: stage === 'done' && !autoRunning,
    progress,
    diarization,
    vision,
    runDiarization,
    runVision,
    revert,
  };
}

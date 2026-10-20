import { useState, useCallback, useRef, useEffect } from 'react';
import {
  CancelSummaryResponse,
  MeetingSummary,
  ProcessTranscriptResponse,
  SummaryProcessResponse,
  Transcript,
} from '@/types';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

import {
  detectAndCacheSummaryLanguage,
  readMeetingSummaryLanguage,
  readCachedDetectedSummaryLanguage,
  readPinnedSummaryLanguageDefault,
} from '@/lib/summary-language-preferences';
import { parseSummaryContent, readSummaryMetadata } from '@/lib/summary-content';
import { useTranslation } from 'react-i18next';
import i18n from '@/i18n';
import { getMeetingSpeakers } from '@/services/forkService';
import { formatTranscriptForSummary, toSpeakerMap, type SpeakerMap } from '@/lib/speakers';

async function resolveSummaryLanguage(
  meetingId: string,
  transcriptTexts: string[]
): Promise<string | null> {
  try {
    const perMeeting = await readMeetingSummaryLanguage(meetingId);
    if (perMeeting.language) return perMeeting.language;
  } catch (err) {
    console.warn('Failed to load meeting summary language:', err);
    toast.warning(i18n.t('summary.language.loadError'), {
      description: i18n.t('summary.language.loadErrorGenerationDescription'),
    });
  }

  try {
    const cachedDetected = await readCachedDetectedSummaryLanguage(meetingId);
    if (cachedDetected) return cachedDetected;
  } catch (err) {
    console.warn('Failed to load cached detected summary language:', err);
  }

  try {
    const detection = await detectAndCacheSummaryLanguage(meetingId, transcriptTexts);
    if (detection.reason === 'tie') {
      toast.warning(i18n.t('summary.language.bilingual'), {
        description: i18n.t('summary.language.bilingualDescription'),
      });
    }
    return detection.language ?? fallbackSummaryLanguage();
  } catch (err) {
    console.warn('Failed to detect transcript summary language:', err);
    return fallbackSummaryLanguage();
  }
}

// Fork: when the language cannot be detected, use the pinned default, else French
// when the interface is in French (summaries are then written natively in French).
function fallbackSummaryLanguage(): string | null {
  return readPinnedSummaryLanguageDefault() ?? (i18n.language === 'fr' ? 'fr' : null);
}

type SummaryStatus = 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';

function restoredSummaryStatus(response?: SummaryProcessResponse | null): SummaryStatus {
  if (!response) return 'idle';
  if (response.status === 'pending' || response.status === 'processing') return 'processing';
  if (response.status === 'error' || response.status === 'failed') return 'error';
  if (parseSummaryContent(response.data)) return 'completed';
  if (response.status === 'completed') return 'error';
  return 'idle';
}

interface UseSummaryGenerationProps {
  initialSummary?: SummaryProcessResponse | null;
  meeting: {
    id: string;
    created_at: string;
  };
  transcripts: Transcript[];
  modelConfig: ModelConfig;
  isModelConfigLoading: boolean;
  selectedTemplate: string;
  onMeetingUpdated?: () => Promise<void>;
  updateMeetingTitle: (title: string) => void;
  setAiSummary: (summary: MeetingSummary | null) => void;
  onOpenModelSettings?: () => void;
}

export function useSummaryGeneration({
  initialSummary,
  meeting,
  transcripts,
  modelConfig,
  isModelConfigLoading,
  selectedTemplate,
  onMeetingUpdated,
  updateMeetingTitle,
  setAiSummary,
  onOpenModelSettings,
}: UseSummaryGenerationProps) {
  const { t } = useTranslation();
  const restored = initialSummary?.meeting_id === meeting.id ? initialSummary : null;
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>(() => restoredSummaryStatus(restored));
  const [summaryError, setSummaryError] = useState<string | null>(() =>
    restoredSummaryStatus(restored) === 'error'
      ? restored?.error || t('summary.generation.failedRetry')
      : null,
  );
  const mountedRef = useRef(true);
  const visibleMeetingIdRef = useRef(meeting.id);
  visibleMeetingIdRef.current = meeting.id;
  const generationIdRef = useRef(0);
  const activeProcessIdRef = useRef<string | null>(null);
  const trackedAttemptRef = useRef<{
    generationId: number;
    startedAt: number;
    provider: ModelConfig['provider'];
    model: ModelConfig['model'];
    finished: boolean;
  } | null>(null);
  const { startSummaryPolling, stopSummaryPolling } = useSidebar();

  const getSummaryStatusMessage = useCallback((status: SummaryStatus) => {
    switch (status) {
      case 'processing':
        return t('summary.status.processing');
      case 'summarizing':
        return t('summary.status.summarizing');
      case 'regenerating':
        return t('summary.status.regenerating');
      case 'completed':
        return t('summary.status.completed');
      case 'error':
        return t('summary.status.error');
      default:
        return '';
    }
  }, [t]);

  const finishGeneration = useCallback(async (
    generationId: number,
    outcome: 'ok' | 'fallback' | 'generation_error' | 'empty_result' | 'cancelled'
  ) => {
    const attempt = trackedAttemptRef.current;
    if (!attempt || attempt.generationId !== generationId || attempt.finished) {
      return;
    }
    attempt.finished = true;
    await Analytics.trackSummaryGenerationCompleted(
      attempt.provider,
      attempt.model,
      outcome === 'ok' || outcome === 'fallback',
      (Date.now() - attempt.startedAt) / 1000,
      outcome === 'ok' ? undefined : outcome,
    );
  }, []);

  const failGeneration = useCallback(async (
    generationId: number,
    isRegeneration: boolean,
    message: string,
    outcome: 'generation_error' | 'empty_result' = 'generation_error',
  ) => {
    if (!mountedRef.current || visibleMeetingIdRef.current !== meeting.id || generationId !== generationIdRef.current) {
      return;
    }
    activeProcessIdRef.current = null;
    setSummaryError(message);
    setSummaryStatus('error');
    toast.error(isRegeneration ? t('summary.generation.regenerateFailed') : t('summary.generation.generateFailed'), {
      description: message,
    });
    await finishGeneration(generationId, outcome);
  }, [finishGeneration, meeting.id, t]);

  const handlePollingResult = useCallback(async (
    pollingResult: SummaryProcessResponse,
    generationId: number,
    isRegeneration: boolean,
  ) => {
    if (!mountedRef.current || visibleMeetingIdRef.current !== meeting.id || generationId !== generationIdRef.current) {
      return;
    }
    if (pollingResult.status === 'cancelled') {
      let existing: SummaryProcessResponse;
      try {
        existing = await invokeTauri<SummaryProcessResponse>('api_get_summary', {
          meetingId: meeting.id,
        });
      } catch (error) {
        console.error('Failed to reload summary after cancellation:', error);
        await failGeneration(generationId, isRegeneration,
          t('summary.generation.cancelledReloadFailed'));
        return;
      }
      if (!mountedRef.current || visibleMeetingIdRef.current !== meeting.id || generationId !== generationIdRef.current) {
        return;
      }
      const restoredSummary = parseSummaryContent(existing.data);
      setAiSummary(restoredSummary);
      setSummaryStatus(restoredSummary ? 'completed' : 'idle');
      setSummaryError(null);
      activeProcessIdRef.current = null;
      await finishGeneration(generationId, 'cancelled');
      return;
    }

    if (pollingResult.status === 'error' || pollingResult.status === 'failed') {
      const errorMessage = pollingResult.error
        || (isRegeneration ? t('summary.generation.regenerationFailed') : t('summary.generation.generationFailed'));
      if (isRegeneration) {
        let existing: SummaryProcessResponse;
        try {
          existing = await invokeTauri<SummaryProcessResponse>('api_get_summary', {
            meetingId: meeting.id,
          });
        } catch (error) {
          console.error('Failed to reload previous summary after generation failure:', error);
          await failGeneration(generationId, isRegeneration,
            t('summary.generation.reloadFailed', { error: errorMessage }));
          return;
        }
        if (!mountedRef.current || visibleMeetingIdRef.current !== meeting.id || generationId !== generationIdRef.current) {
          return;
        }
        const restoredSummary = parseSummaryContent(existing.data);
        if (restoredSummary) {
          setAiSummary(restoredSummary);
          setSummaryStatus('completed');
          setSummaryError(null);
          toast.error(t('summary.generation.regenerateFailed'), {
            description: t('summary.generation.previousRestored', { error: errorMessage }),
          });
          activeProcessIdRef.current = null;
          await finishGeneration(generationId, 'generation_error');
          return;
        }
      }
      await failGeneration(generationId, isRegeneration, errorMessage);
      return;
    }

    if (pollingResult.status === 'completed') {
      const summary = parseSummaryContent(pollingResult.data);
      if (!summary) {
        await failGeneration(generationId, isRegeneration,
          t('summary.generation.emptyResult'),
          'empty_result',
        );
        return;
      }
      const metadata = readSummaryMetadata(pollingResult.data);
      const meetingName = metadata.meetingName || pollingResult.meetingName;
      if (meetingName) {
        updateMeetingTitle(meetingName);
      }
      setAiSummary(summary);
      setSummaryStatus('completed');
      activeProcessIdRef.current = null;
      setSummaryError(null);
      if (metadata.normalizationFallback) {
        toast.warning(t('summary.generation.fallbackTitle'), {
          description: t('summary.generation.fallbackDescription'),
        });
      } else {
        toast.success(t('summary.generation.success'), {
          description: metadata.reasoningStripped
            ? t('summary.generation.readyReasoningStripped')
            : t('summary.generation.ready'),
          duration: 4000,
        });
      }
      if (meetingName && onMeetingUpdated) {
        await onMeetingUpdated();
      }
      await finishGeneration(
        generationId,
        metadata.normalizationFallback ? 'fallback' : 'ok',
      );
    }
  }, [failGeneration, finishGeneration, meeting.id, onMeetingUpdated, setAiSummary, updateMeetingTitle, t]);

  // Keep polling attached across ordinary rerenders without retaining stale view callbacks.
  const pollingResultRef = useRef(handlePollingResult);
  pollingResultRef.current = handlePollingResult;

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (activeProcessIdRef.current) {
        stopSummaryPolling(meeting.id, activeProcessIdRef.current);
      }
    };
  }, [meeting.id, stopSummaryPolling]);

  useEffect(() => {
    if (!initialSummary || initialSummary.meeting_id !== meeting.id) return;
    const status = restoredSummaryStatus(initialSummary);
    setSummaryStatus(status);
    setSummaryError(status === 'error'
      ? initialSummary.error || i18n.t('summary.generation.failedRetry')
      : null);
    if (status !== 'processing' || !initialSummary.start) return;

    const generationId = ++generationIdRef.current;
    activeProcessIdRef.current = initialSummary.start;
    const isRegeneration = !!parseSummaryContent(initialSummary.data);
    startSummaryPolling(meeting.id, initialSummary.start, result =>
      pollingResultRef.current(result, generationId, isRegeneration));
  }, [initialSummary, meeting.id, startSummaryPolling]);

  const processSummary = useCallback(async ({
    transcriptText,
    transcriptTexts,
    customPrompt = '',
    isRegeneration = false,
  }: {
    transcriptText: string;
    transcriptTexts?: string[];
    customPrompt?: string;
    isRegeneration?: boolean;
  }) => {
    const previousAttempt = trackedAttemptRef.current;
    if (previousAttempt && !previousAttempt.finished) {
      await finishGeneration(previousAttempt.generationId, 'cancelled');
    }

    const generationId = ++generationIdRef.current;
    activeProcessIdRef.current = null;
    setSummaryStatus(isRegeneration ? 'regenerating' : 'processing');
    setSummaryError(null);

    try {
      if (!transcriptText.trim()) {
        await failGeneration(generationId, isRegeneration, t('summary.generation.noTranscriptText'), 'empty_result');
        return;
      }

      const timeSinceRecording = (Date.now() - new Date(meeting.created_at).getTime()) / 60000;
      trackedAttemptRef.current = {
        generationId,
        startedAt: Date.now(),
        provider: modelConfig.provider,
        model: modelConfig.model,
        finished: false,
      };
      await Analytics.trackSummaryGenerationStarted(
        modelConfig.provider,
        modelConfig.model,
        transcriptText.length,
        timeSinceRecording,
      );
      if (customPrompt.trim()) {
        await Analytics.trackCustomPromptUsed(customPrompt.trim().length);
      }
      toast.info(isRegeneration ? t('summary.status.regenerating') : t('summary.status.summarizing'), {
        description: t('summary.generation.usingModel', { provider: modelConfig.provider, model: modelConfig.model }),
        duration: 3000,
      });

      const summaryLanguage = await resolveSummaryLanguage(
        meeting.id,
        transcriptTexts?.length ? transcriptTexts : [transcriptText],
      );
      if (!mountedRef.current || visibleMeetingIdRef.current !== meeting.id || generationId !== generationIdRef.current) {
        return;
      }

      const result = await invokeTauri<ProcessTranscriptResponse>('api_process_transcript', {
        text: transcriptText,
        model: modelConfig.provider,
        modelName: modelConfig.model,
        meetingId: meeting.id,
        chunkSize: 40000,
        overlap: 1000,
        customPrompt,
        templateId: selectedTemplate,
        summaryLanguage,
      });
      const processId = result.process_id;
      if (!mountedRef.current || visibleMeetingIdRef.current !== meeting.id) return;
      if (generationId !== generationIdRef.current) {
        await invokeTauri('api_cancel_summary', { meetingId: meeting.id, processId }).catch(() => {});
        return;
      }
      activeProcessIdRef.current = processId;

      startSummaryPolling(meeting.id, processId, result =>
        pollingResultRef.current(result, generationId, isRegeneration));
    } catch (error) {
      await failGeneration(generationId, isRegeneration, error instanceof Error ? error.message : t('summary.generation.failed'));
    }
  }, [
    failGeneration,
    finishGeneration,
    meeting.created_at,
    meeting.id,
    modelConfig,
    onMeetingUpdated,
    selectedTemplate,
    setAiSummary,
    startSummaryPolling,
    updateMeetingTitle,
    t,
  ]);

  // Helper function to fetch ALL transcripts for summary generation
  const fetchAllTranscripts = useCallback(async (meetingId: string): Promise<Transcript[]> => {
    try {
      console.log('📊 Fetching all transcripts for meeting:', meetingId);

      // First, get total count by fetching first page
      const firstPage = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: 1,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      const totalCount = firstPage.total_count;
      console.log(`📊 Total transcripts in database: ${totalCount}`);

      if (totalCount === 0) {
        return [];
      }

      // Fetch all transcripts in one call
      const allData = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: totalCount,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      console.log(`✅ Fetched ${allData.transcripts.length} transcripts from database`);
      return allData.transcripts;
    } catch (error) {
      console.error('❌ Error fetching all transcripts:', error);
      toast.error(t('summary.generation.fetchTranscriptsError'));
      return [];
    }
  }, [t]);

  // Fork: transcript lines carry speaker names ("[mm:ss] Marie : ...") when known
  const buildSummaryTranscriptPayload = useCallback((allTranscripts: Transcript[], speakers: SpeakerMap = {}) => {
    return {
      transcriptText: formatTranscriptForSummary(allTranscripts, speakers, t),
      transcriptTexts: allTranscripts.map((transcript) => transcript.text),
    };
  }, [t]);

  const loadSpeakers = useCallback(async (meetingId: string): Promise<SpeakerMap> => {
    try {
      return toSpeakerMap(await getMeetingSpeakers(meetingId));
    } catch (error) {
      console.warn('Could not load speaker names for the summary:', error);
      return {};
    }
  }, []);

  const showPreflightError = useCallback((message: string) => {
    setSummaryError(message);
    setSummaryStatus('error');
    toast.error(message);
  }, []);

  const handleGenerateSummary = useCallback(async (customPrompt: string = '') => {
    if (isModelConfigLoading) {
      toast.info(t('summary.generation.loadingConfig'));
      return;
    }
    const allTranscripts = await fetchAllTranscripts(meeting.id);
    if (!allTranscripts.length) {
      showPreflightError(t('summary.generation.noTranscripts'));
      return;
    }

    try {
      if (modelConfig.provider === 'ollama') {
        const models = await invokeTauri<unknown[]>('get_ollama_models', {
          endpoint: modelConfig.ollamaEndpoint || null,
        });
        if (models.length === 0) {
          showPreflightError(t('summary.generation.noOllamaModels'));
          return;
        }
      }
      if (modelConfig.provider === 'builtin-ai') {
        if (!modelConfig.model) {
          showPreflightError(t('summary.generation.noBuiltinModel'));
          onOpenModelSettings?.();
          return;
        }
        const isReady = await invokeTauri<boolean>('builtin_ai_is_model_ready', {
          modelName: modelConfig.model,
          refresh: true,
        });
        if (!isReady) {
          showPreflightError(t('summary.generation.builtinNotReady'));
          onOpenModelSettings?.();
          return;
        }
      }
    } catch (error) {
      console.error('Failed to validate summary model:', error);
      showPreflightError(t('summary.generation.validateFailed'));
      return;
    }

    await processSummary({
      ...buildSummaryTranscriptPayload(allTranscripts, await loadSpeakers(meeting.id)),
      customPrompt,
    });
  }, [
    buildSummaryTranscriptPayload,
    loadSpeakers,
    fetchAllTranscripts,
    isModelConfigLoading,
    meeting.id,
    modelConfig,
    onOpenModelSettings,
    processSummary,
    showPreflightError,
    t,
  ]);

  // Public API: Regenerate summary from the current saved transcript
  const handleRegenerateSummary = useCallback(async () => {
    const allTranscripts = await fetchAllTranscripts(meeting.id);

    if (!allTranscripts.length) {
      console.error('No transcripts available for regeneration');
      toast.error(t('summary.generation.noTranscriptsRegenerate'));
      return;
    }

    await processSummary({
      ...buildSummaryTranscriptPayload(allTranscripts, await loadSpeakers(meeting.id)),
      isRegeneration: true
    });
  }, [meeting.id, fetchAllTranscripts, buildSummaryTranscriptPayload, loadSpeakers, processSummary, t]);

  // Public API: Stop ongoing summary generation
  const handleStopGeneration = useCallback(async () => {
    const generationId = generationIdRef.current;
    const processId = activeProcessIdRef.current;
    if (!processId) {
      generationIdRef.current += 1;
      activeProcessIdRef.current = null;
      setSummaryStatus('idle');
      setSummaryError(null);
      await finishGeneration(generationId, 'cancelled');
      toast.info(t('summary.generation.stopped'), {
        description: t('summary.generation.stoppedDescription'),
        duration: 3000,
      });
      return;
    }

    try {
      const result = await invokeTauri<CancelSummaryResponse>('api_cancel_summary', {
        meetingId: meeting.id,
        processId,
      });
      if (!mountedRef.current || visibleMeetingIdRef.current !== meeting.id || generationId !== generationIdRef.current) {
        return;
      }
      if (result.cancelled) {
        generationIdRef.current += 1;
        activeProcessIdRef.current = null;
        stopSummaryPolling(meeting.id, processId);
        setSummaryStatus('idle');
        setSummaryError(null);
        await finishGeneration(generationId, 'cancelled');
        toast.info(t('summary.generation.stopped'), {
          description: t('summary.generation.stoppedDescription'),
          duration: 3000,
        });
      } else if (activeProcessIdRef.current === processId) {
        toast.info(t('summary.generation.alreadyFinishing'), {
          description: t('summary.generation.alreadyFinishingDescription'),
        });
      }
    } catch (error) {
      console.error('Failed to cancel summary generation:', error);
      if (
        generationId === generationIdRef.current
        && activeProcessIdRef.current === processId
      ) {
        toast.error(t('summary.generation.stopFailed'), {
          description: t('summary.generation.stopFailedDescription'),
        });
      }
    }
  }, [finishGeneration, meeting.id, stopSummaryPolling, t]);

  return {
    summaryStatus,
    summaryError,
    handleGenerateSummary,
    handleRegenerateSummary,
    handleStopGeneration,
    getSummaryStatusMessage,
  };
}

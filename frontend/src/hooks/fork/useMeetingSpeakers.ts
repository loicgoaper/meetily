'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import {
  getMeetingSpeakers,
  mergeSpeakers,
  renameSpeaker as renameSpeakerCmd,
  setSegmentSpeaker,
} from '@/services/forkService';
import { toSpeakerMap, type SpeakerMap } from '@/lib/speakers';

/**
 * Speaker names of a meeting, with the corrections made from the transcript:
 * rename, "speaker 4 is speaker 1" (all lines) or reassign a single line.
 * Reloads after an identification run.
 */
export function useMeetingSpeakers(meetingId: string, onTranscriptsChanged?: () => Promise<void> | void) {
  const { t } = useTranslation();
  const [speakers, setSpeakers] = useState<SpeakerMap>({});
  const onChangedRef = useRef(onTranscriptsChanged);
  onChangedRef.current = onTranscriptsChanged;

  const reload = useCallback(async () => {
    try {
      setSpeakers(toSpeakerMap(await getMeetingSpeakers(meetingId)));
    } catch (error) {
      console.warn('Could not load speakers:', error);
    }
  }, [meetingId]);

  useEffect(() => {
    void reload();
    const unlisten = listen<{ meetingId: string; stage: string }>('diarization-progress', event => {
      if (event.payload.meetingId === meetingId && event.payload.stage === 'complete') void reload();
    });
    return () => {
      unlisten.then(fn => fn());
    };
  }, [meetingId, reload]);

  const renameSpeaker = useCallback(
    async (speakerKey: string, displayName: string | null) => {
      try {
        await renameSpeakerCmd(meetingId, speakerKey, displayName);
        await reload();
      } catch (error) {
        toast.error(String(error));
      }
    },
    [meetingId, reload],
  );

  const mergeSpeaker = useCallback(
    async (fromKey: string, intoKey: string) => {
      try {
        const moved = await mergeSpeakers(meetingId, fromKey, intoKey);
        await reload();
        await onChangedRef.current?.();
        toast.success(t('fork.speakers.merged', { count: moved }));
      } catch (error) {
        toast.error(String(error));
      }
    },
    [meetingId, reload, t],
  );

  const reassignSegment = useCallback(
    async (segmentId: string, intoKey: string) => {
      try {
        await setSegmentSpeaker(segmentId, intoKey);
        await onChangedRef.current?.();
        toast.success(t('fork.speakers.reassigned'));
      } catch (error) {
        toast.error(String(error));
      }
    },
    [t],
  );

  return { speakers, reload, renameSpeaker, mergeSpeaker, reassignSegment };
}

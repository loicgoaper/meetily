'use client';

import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { getMeetingSpeakers, renameSpeaker as renameSpeakerCmd } from '@/services/forkService';
import { toSpeakerMap, type SpeakerMap } from '@/lib/speakers';

/** Speaker names of a meeting; reloads after an identification run. */
export function useMeetingSpeakers(meetingId: string) {
  const [speakers, setSpeakers] = useState<SpeakerMap>({});

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

  return { speakers, reload, renameSpeaker };
}

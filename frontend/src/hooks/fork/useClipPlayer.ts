'use client';

import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { getAudioClip } from '@/services/forkService';

// One shared player: starting a clip stops the previous one.
let audio: HTMLAudioElement | null = null;
let playingKey: string | null = null;
const listeners = new Set<(key: string | null) => void>();

function setPlaying(key: string | null) {
  playingKey = key;
  listeners.forEach(fn => fn(key));
}

export function stopClip() {
  audio?.pause();
  setPlaying(null);
}

/** Play a passage of a meeting's recording (transcript segment replay). */
export function useClipPlayer() {
  const [current, setCurrent] = useState<string | null>(playingKey);
  const [loadingKey, setLoadingKey] = useState<string | null>(null);

  useEffect(() => {
    listeners.add(setCurrent);
    return () => {
      listeners.delete(setCurrent);
    };
  }, []);

  const play = useCallback(async (meetingId: string, start: number, end: number, key: string) => {
    if (playingKey === key) {
      stopClip();
      return;
    }
    stopClip();
    setLoadingKey(key);
    try {
      const url = await getAudioClip(meetingId, start, end);
      if (!audio) audio = new Audio();
      audio.src = url;
      audio.onended = () => setPlaying(null);
      await audio.play();
      setPlaying(key);
    } catch (error) {
      toast.error(String(error));
      setPlaying(null);
    } finally {
      setLoadingKey(null);
    }
  }, []);

  return { play, stop: stopClip, playingKey: current, loadingKey };
}

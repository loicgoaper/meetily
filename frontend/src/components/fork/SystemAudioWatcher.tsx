'use client';

import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { useRouter } from 'next/navigation';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';

/**
 * Warns during a video-call recording when the recorded system output stays silent
 * (the meeting is probably heard on another output: headset, screen...).
 */
export function SystemAudioWatcher() {
  const { t } = useTranslation();
  const router = useRouter();

  useEffect(() => {
    const unlisten = listen<{ device: string | null; alternative: string | null }>('system-audio-silent', event => {
      const device = event.payload.device ?? '?';
      const alternative = event.payload.alternative;
      toast.warning(t('fork.systemAudio.silentTitle'), {
        description: alternative
          ? t('fork.systemAudio.silentWithAlternative', { device, alternative })
          : t('fork.systemAudio.silent', { device }),
        duration: 60000,
        action: { label: t('fork.systemAudio.openSettings'), onClick: () => router.push('/settings') },
      });
    });
    return () => {
      unlisten.then(fn => fn());
    };
  }, [t, router]);

  return null;
}

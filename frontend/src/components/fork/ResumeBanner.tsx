'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { getActiveResume, type ResumeTarget } from '@/services/forkService';
import { formatElapsed } from '@/lib/fork/resume';

/** Above the recording controls while a stopped meeting is being resumed. */
export function ResumeBanner() {
  const { t } = useTranslation();
  const [target, setTarget] = useState<ResumeTarget | null>(null);

  useEffect(() => {
    const refresh = () => getActiveResume().then(setTarget).catch(() => setTarget(null));
    refresh();
    const started = listen('recording-started', refresh);
    const stopped = listen('recording-stopped', () => setTarget(null));
    const failed = listen<{ message: string }>('resume-merge-failed', event =>
      toast.error(t('fork.resume.mergeFailed'), { description: event.payload.message, duration: 15000 }),
    );
    return () => {
      [started, stopped, failed].forEach(p => p.then(fn => fn()));
    };
  }, [t]);

  if (!target) return null;
  return (
    <div className="pointer-events-none fixed bottom-32 left-0 right-0 z-10 flex justify-center">
      <div className="rounded-full bg-amber-50 px-4 py-1 text-xs text-amber-800 shadow">
        {t('fork.resume.banner', { title: target.title, time: formatElapsed(target.offsetSeconds) })}
      </div>
    </div>
  );
}

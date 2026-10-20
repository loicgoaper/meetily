'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import { getMeetingContext, saveMeetingContext, type MeetingContext } from '@/services/forkService';

const SAVE_DELAY_MS = 800;

/** Persisted context of a meeting, saved automatically shortly after each change. */
export function useMeetingContextData(meetingId: string) {
  const { t } = useTranslation();
  const [context, setContext] = useState<MeetingContext>({ participants: [] });
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latest = useRef<MeetingContext>(context);
  // The user edited before the saved context arrived: keep the edit
  const dirty = useRef(false);

  useEffect(() => {
    let cancelled = false;
    dirty.current = false;
    setLoaded(false);
    getMeetingContext(meetingId)
      .then(ctx => {
        if (cancelled || dirty.current) return;
        setContext(ctx);
        latest.current = ctx;
      })
      .catch(error => console.warn('Could not load meeting context:', error))
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [meetingId]);

  /** Save now if an edit is waiting (call before generating a summary). */
  const flush = useCallback(async () => {
    if (!timer.current) return;
    clearTimeout(timer.current);
    timer.current = null;
    setSaving(true);
    try {
      const ctx = latest.current;
      await saveMeetingContext(meetingId, { ...ctx, participants: ctx.participants.filter(p => p.name.trim()) });
    } catch (error) {
      console.error('Failed to save meeting context:', error);
      toast.error(t('fork.common.saveError'));
    } finally {
      setSaving(false);
    }
  }, [meetingId, t]);

  const update = useCallback(
    (next: MeetingContext) => {
      dirty.current = true;
      setContext(next);
      latest.current = next;
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => void flush(), SAVE_DELAY_MS);
    },
    [flush],
  );

  // Save pending edits when leaving the meeting
  useEffect(() => () => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
      const ctx = latest.current;
      void saveMeetingContext(meetingId, { ...ctx, participants: ctx.participants.filter(p => p.name.trim()) }).catch(() => undefined);
    }
  }, [meetingId]);

  return { context, loaded, saving, update, flush };
}

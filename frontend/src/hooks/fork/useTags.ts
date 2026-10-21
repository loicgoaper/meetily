'use client';

import { useCallback, useEffect, useState } from 'react';
import { getAllMeetingTags, listTags, TAGS_CHANGED_EVENT, type Tag } from '@/services/forkService';

/** All tags + meeting assignments; refreshed whenever tags change anywhere in the app. */
export function useTags() {
  const [tags, setTags] = useState<Tag[]>([]);
  const [assignments, setAssignments] = useState<Record<string, string[]>>({});

  const reload = useCallback(async () => {
    try {
      const [list, map] = await Promise.all([listTags(), getAllMeetingTags()]);
      setTags(list);
      setAssignments(map);
    } catch (error) {
      console.warn('Could not load tags:', error);
    }
  }, []);

  useEffect(() => {
    void reload();
    const onChange = () => void reload();
    window.addEventListener(TAGS_CHANGED_EVENT, onChange);
    return () => window.removeEventListener(TAGS_CHANGED_EVENT, onChange);
  }, [reload]);

  const tagsFor = useCallback(
    (meetingId: string) => (assignments[meetingId] ?? []).map(id => tags.find(t => t.id === id)).filter((t): t is Tag => !!t),
    [assignments, tags],
  );

  return { tags, assignments, tagsFor, reload };
}

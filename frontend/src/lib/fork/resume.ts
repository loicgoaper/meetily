// Meetily perso: resume a stopped meeting from its page. The request crosses the
// navigation to the recording page through sessionStorage, like the sidebar start.

const RESUME_KEY = 'fork.resumeMeeting';

export interface ResumeRequest {
  meetingId: string;
  title: string;
}

type Writable = Pick<Storage, 'setItem'>;
type Readable = Pick<Storage, 'getItem' | 'removeItem'>;

/** Ask the recording page to resume this meeting (picked up by its auto-start). */
export function requestResume(request: ResumeRequest, storage: Writable = sessionStorage) {
  storage.setItem(RESUME_KEY, JSON.stringify(request));
  storage.setItem('autoStartRecording', 'true');
}

/** The pending resume request, consumed. */
export function takeResumeRequest(storage: Readable = sessionStorage): ResumeRequest | null {
  const raw = storage.getItem(RESUME_KEY);
  storage.removeItem(RESUME_KEY);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    return typeof parsed?.meetingId === 'string' && typeof parsed?.title === 'string'
      ? { meetingId: parsed.meetingId, title: parsed.title }
      : null;
  } catch {
    return null;
  }
}

/** Meeting page source after a stop: a resumed meeting regenerates its summary. */
export function sourceAfterSave(response: { resumed?: boolean }): 'recording' | 'resume' {
  return response.resumed ? 'resume' : 'recording';
}

/** The automatic summary runs on a new meeting, or replaces the one of a resumed meeting. */
export function canAutoGenerate(status: string, resumed: boolean): boolean {
  return status === 'idle' || (resumed && (status === 'completed' || status === 'error'));
}

/** "[mm:ss]" of the meeting timeline (minutes keep counting past an hour). */
export function formatElapsed(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
}

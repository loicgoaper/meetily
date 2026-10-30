import { describe, expect, test } from 'bun:test';
import { canAutoGenerate, formatElapsed, requestResume, sourceAfterSave, takeResumeRequest } from '@/lib/fork/resume';

function memoryStorage() {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
  };
}

describe('resume a stopped meeting', () => {
  test('the request is read once and arms the auto-start', () => {
    const storage = memoryStorage();
    requestResume({ meetingId: 'meeting-1', title: 'Point client' }, storage);
    expect(storage.data.get('autoStartRecording')).toBe('true');
    expect(takeResumeRequest(storage)).toEqual({ meetingId: 'meeting-1', title: 'Point client' });
    expect(takeResumeRequest(storage)).toBeNull();
  });

  test('a damaged request is ignored', () => {
    const storage = memoryStorage();
    storage.setItem('fork.resumeMeeting', '{oops');
    expect(takeResumeRequest(storage)).toBeNull();
  });

  test('a resumed save opens the meeting as resumed', () => {
    expect(sourceAfterSave({ resumed: true })).toBe('resume');
    expect(sourceAfterSave({})).toBe('recording');
  });

  test('the automatic summary replaces the summary of a resumed meeting only', () => {
    expect(canAutoGenerate('idle', false)).toBe(true);
    expect(canAutoGenerate('completed', false)).toBe(false);
    expect(canAutoGenerate('completed', true)).toBe(true);
    expect(canAutoGenerate('error', true)).toBe(true);
    expect(canAutoGenerate('processing', true)).toBe(false);
  });

  test('elapsed time continues past an hour', () => {
    expect(formatElapsed(2112)).toBe('35:12');
    expect(formatElapsed(3792.7)).toBe('63:12');
  });
});

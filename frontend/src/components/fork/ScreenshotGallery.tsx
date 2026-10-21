'use client';

import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Eye, EyeOff } from 'lucide-react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  formatTimestamp,
  getScreenNotes,
  getScreenshot,
  listMeetingScreenshots,
  setScreenshotExcluded,
  type ScreenFrame,
  type ScreenNote,
} from '@/services/forkService';

function Thumbnail({ meetingId, frame, onOpen, onToggle }: {
  meetingId: string;
  frame: ScreenFrame;
  onOpen: () => void;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    getScreenshot(meetingId, frame.file, true)
      .then(url => alive && setSrc(url))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [meetingId, frame.file]);

  return (
    <div className={`group relative overflow-hidden rounded-md border ${frame.excluded ? 'border-dashed border-gray-300 opacity-50' : 'border-gray-200'}`}>
      <button type="button" onClick={onOpen} className="block w-full bg-gray-100 aspect-video">
        {src && <img src={src} alt={formatTimestamp(frame.t)} className="h-full w-full object-cover" />}
      </button>
      <div className="flex items-center justify-between px-1.5 py-1 text-[11px] text-gray-600">
        <span>
          [{formatTimestamp(frame.t)}]
          {frame.monitor != null && <span className="ml-1 rounded bg-gray-100 px-1 text-gray-500">{t('fork.screens.monitor', { n: frame.monitor })}</span>}
        </span>
        <button
          type="button"
          onClick={onToggle}
          title={frame.excluded ? t('fork.screens.include') : t('fork.screens.exclude')}
          className="text-gray-500 hover:text-gray-900"
        >
          {frame.excluded ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
        </button>
      </div>
    </div>
  );
}

/** Screenshots kept during the meeting, with what the vision model read on them. */
export function ScreenshotGallery({ meetingId, refreshKey }: { meetingId: string; refreshKey?: number }) {
  const { t } = useTranslation();
  const [frames, setFrames] = useState<ScreenFrame[]>([]);
  const [notes, setNotes] = useState<ScreenNote[]>([]);
  const [opened, setOpened] = useState<{ frame: ScreenFrame; src: string | null } | null>(null);

  const load = useCallback(async () => {
    try {
      const [f, n] = await Promise.all([listMeetingScreenshots(meetingId), getScreenNotes(meetingId)]);
      setFrames(f);
      setNotes(n);
    } catch (error) {
      console.warn('Could not load screenshots:', error);
    }
  }, [meetingId]);

  useEffect(() => {
    void load();
  }, [load, refreshKey]);

  const toggle = async (frame: ScreenFrame) => {
    try {
      await setScreenshotExcluded(meetingId, frame.file, !frame.excluded);
      setFrames(prev => prev.map(f => (f.file === frame.file ? { ...f, excluded: !f.excluded } : f)));
    } catch (error) {
      toast.error(String(error));
    }
  };

  const open = async (frame: ScreenFrame) => {
    setOpened({ frame, src: null });
    try {
      const src = await getScreenshot(meetingId, frame.file, false);
      setOpened({ frame, src });
    } catch (error) {
      toast.error(String(error));
    }
  };

  if (frames.length === 0) {
    return <div className="p-4 text-sm text-gray-500">{t('fork.screens.none')}</div>;
  }

  const noteFor = (file: string) => notes.find(n => n.file === file && n.content.trim() !== '=');

  return (
    <div className="space-y-3 p-3">
      <p className="text-xs text-gray-500">{t('fork.screens.hint')}</p>
      <div className="grid grid-cols-2 @lg:grid-cols-3 gap-2">
        {frames.map(frame => (
          <Thumbnail key={frame.file} meetingId={meetingId} frame={frame} onOpen={() => open(frame)} onToggle={() => toggle(frame)} />
        ))}
      </div>

      <Dialog open={!!opened} onOpenChange={o => !o && setOpened(null)}>
        <DialogContent className="max-w-5xl">
          <DialogHeader>
            <DialogTitle>
              [{opened ? formatTimestamp(opened.frame.t) : ''}] {opened && noteFor(opened.frame.file)?.title}
            </DialogTitle>
          </DialogHeader>
          {opened?.src && <img src={opened.src} alt="" className="w-full rounded-md border" />}
          {opened && noteFor(opened.frame.file) && (
            <p className="whitespace-pre-wrap text-sm text-gray-700">{noteFor(opened.frame.file)?.content}</p>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

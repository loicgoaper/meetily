'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { NotebookPen } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useMeetingSetup } from '@/contexts/MeetingSetupContext';
import { getScreenCaptureStatus } from '@/services/forkService';

/** Round button in the recording pill: edit the live context; shows screenshot count. */
export function LiveContextButton({ disabled }: { disabled?: boolean }) {
  const { t } = useTranslation();
  const { openLiveEditor } = useMeetingSetup();
  const [capture, setCapture] = useState<{ active: boolean; frames: number }>({ active: false, frames: 0 });

  useEffect(() => {
    getScreenCaptureStatus().then(setCapture).catch(() => undefined);
    const unlistenStatus = listen<{ active: boolean }>('screen-capture-status', event =>
      setCapture(prev => ({ ...prev, active: event.payload.active })),
    );
    const unlistenFrame = listen<{ count: number }>('screen-capture-frame', event =>
      setCapture({ active: true, frames: event.payload.count }),
    );
    return () => {
      unlistenStatus.then(fn => fn());
      unlistenFrame.then(fn => fn());
    };
  }, []);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          onClick={openLiveEditor}
          disabled={disabled}
          className="w-10 h-10 flex items-center justify-center bg-white border-2 border-gray-300 text-gray-600 hover:border-gray-400 hover:bg-gray-50 rounded-full transition-colors relative disabled:opacity-50"
        >
          <NotebookPen size={16} />
          {capture.active && (
            <span className="absolute -top-1 -right-1 min-w-[18px] h-[18px] rounded-full bg-blue-600 px-1 text-[10px] font-semibold leading-[18px] text-white">
              {capture.frames}
            </span>
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent>
        <p>{capture.active ? t('fork.setup.liveButtonCapturing', { count: capture.frames }) : t('fork.setup.liveButton')}</p>
      </TooltipContent>
    </Tooltip>
  );
}

'use client';

import { useRouter } from 'next/navigation';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { PlayCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { checkResume } from '@/services/forkService';
import { requestResume } from '@/lib/fork/resume';

/** "Resume meeting": records a new session appended to this meeting. */
export function ResumeMeetingButton({
  meetingId,
  meetingTitle,
  busyReason,
}: {
  meetingId: string;
  meetingTitle: string;
  busyReason: string | null;
}) {
  const { t } = useTranslation();
  const router = useRouter();
  const { isRecording } = useRecordingState();
  const reason = isRecording ? t('fork.resume.recordingInProgress') : busyReason;

  const resume = async () => {
    try {
      await checkResume(meetingId);
    } catch (error) {
      toast.error(String(error));
      return;
    }
    requestResume({ meetingId, title: meetingTitle });
    router.push('/');
  };

  const button = (
    <Button size="sm" variant="outline" className="h-7 gap-1 text-xs" disabled={!!reason} onClick={resume}>
      <PlayCircle className="h-3.5 w-3.5" />
      {t('fork.resume.button')}
    </Button>
  );
  if (!reason) return button;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span>{button}</span>
      </TooltipTrigger>
      <TooltipContent>
        <p>{reason}</p>
      </TooltipContent>
    </Tooltip>
  );
}

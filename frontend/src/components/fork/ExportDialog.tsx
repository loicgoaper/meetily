'use client';

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Download } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { exportMeeting, type ExportParts } from '@/services/forkService';

const PARTS: (keyof ExportParts)[] = ['summary', 'transcript', 'audio', 'context', 'screenshots'];

/** Export a meeting: one file when one part is chosen, a zip otherwise. */
export function ExportButton({ meetingId, hasScreenshots }: { meetingId: string; hasScreenshots: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [parts, setParts] = useState<ExportParts>({ summary: true, transcript: true, audio: true, context: false, screenshots: false });
  const visible = PARTS.filter(p => p !== 'screenshots' || hasScreenshots);
  const count = visible.filter(p => parts[p]).length;

  const run = async () => {
    setBusy(true);
    try {
      const saved = await exportMeeting(meetingId, { ...parts, screenshots: hasScreenshots && parts.screenshots });
      if (saved) {
        toast.success(t('fork.export.done'), { description: saved });
        setOpen(false);
      }
    } catch (error) {
      toast.error(String(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button variant="ghost" size="sm" onClick={() => setOpen(true)} title={t('fork.export.hint')}>
        <Download /> {t('fork.export.button')}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t('fork.export.title')}</DialogTitle>
            <DialogDescription>{t('fork.export.description')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            {visible.map(part => (
              <label key={part} className="flex items-center gap-3 rounded-md border border-gray-200 px-3 py-2 text-sm hover:bg-gray-50">
                <input type="checkbox" checked={parts[part]} onChange={e => setParts({ ...parts, [part]: e.target.checked })} />
                <span>
                  <span className="block font-medium text-gray-900">{t(`fork.export.parts.${part}`)}</span>
                  <span className="block text-xs text-gray-500">{t(`fork.export.partsHint.${part}`)}</span>
                </span>
              </label>
            ))}
          </div>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setOpen(false)}>{t('fork.common.cancel')}</Button>
            <Button onClick={run} disabled={busy || count === 0}>
              {count > 1 ? t('fork.export.asZip') : t('fork.export.asFile')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

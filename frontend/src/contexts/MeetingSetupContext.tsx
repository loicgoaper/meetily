'use client';

import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { MeetingContextForm } from '@/components/fork/MeetingContextForm';
import {
  getPreferences,
  PREF,
  setLiveMeetingContext,
  startScreenCapture,
  stopScreenCapture,
  type MeetingContext,
} from '@/services/forkService';

export interface MeetingSetupResult {
  /** Title typed by the user, or null to keep the generated one */
  title: string | null;
  context: MeetingContext;
}

interface MeetingSetupContextValue {
  /** Ask for title + context before a recording; null if the user cancelled. */
  requestSetup: () => Promise<MeetingSetupResult | null>;
  /** Context of the recording in progress (null when not recording). */
  liveContext: MeetingContext | null;
  /** Edit the context of the recording in progress. */
  openLiveEditor: () => void;
}

const MeetingSetupContext = createContext<MeetingSetupContextValue | null>(null);

export function useMeetingSetup() {
  const ctx = useContext(MeetingSetupContext);
  if (!ctx) throw new Error('useMeetingSetup must be used within MeetingSetupProvider');
  return ctx;
}

const SELF_NAME_KEY = 'fork.selfName';

function readSelfName(): string {
  try {
    return window.localStorage.getItem(SELF_NAME_KEY) ?? '';
  } catch {
    return '';
  }
}

function rememberSelfName(context: MeetingContext) {
  const self = context.participants.find(p => p.isSelf && p.name.trim());
  if (!self) return;
  try {
    window.localStorage.setItem(SELF_NAME_KEY, self.name.trim());
  } catch {
    // convenience only
  }
}

function cleanContext(context: MeetingContext): MeetingContext {
  return { ...context, participants: context.participants.filter(p => p.name.trim()) };
}

export function MeetingSetupProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<'setup' | 'live' | null>(null);
  const [title, setTitle] = useState('');
  const [draft, setDraft] = useState<MeetingContext>({ participants: [] });
  const [liveContext, setLiveContext] = useState<MeetingContext | null>(null);
  const resolverRef = useRef<((result: MeetingSetupResult | null) => void) | null>(null);

  // Forget the live context once the recording is over (the backend saved it with the meeting)
  useEffect(() => {
    const unlisten = listen('recording-stopped', () => setLiveContext(null));
    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  const requestSetup = useCallback(async () => {
    // A second start request (button + sidebar) cancels the first one instead of leaving it pending
    resolverRef.current?.(null);
    resolverRef.current = null;
    let captureDefault = false;
    try {
      const prefs = await getPreferences();
      captureDefault = prefs[PREF.screenDefaultEnabled] === 'true';
    } catch {
      // defaults below
    }
    const selfName = readSelfName();
    setTitle('');
    setDraft({
      meetingType: 'visio',
      participants: selfName ? [{ name: selfName, isSelf: true }] : [],
      captureScreen: captureDefault,
    });
    setMode('setup');
    return new Promise<MeetingSetupResult | null>(resolve => {
      resolverRef.current = resolve;
    });
  }, []);

  const finishSetup = (result: MeetingSetupResult | null) => {
    resolverRef.current?.(result);
    resolverRef.current = null;
    setMode(null);
    if (result) {
      rememberSelfName(result.context);
      setLiveContext(result.context);
    }
  };

  const openLiveEditor = useCallback(() => {
    setDraft(liveContext ?? { meetingType: 'visio', participants: [] });
    setMode('live');
  }, [liveContext]);

  const saveLive = async () => {
    const context = cleanContext(draft);
    try {
      await setLiveMeetingContext(context);
      if (!!context.captureScreen !== !!liveContext?.captureScreen) {
        if (context.captureScreen) await startScreenCapture();
        else await stopScreenCapture();
      }
      setLiveContext(context);
      rememberSelfName(context);
      setMode(null);
      toast.success(t('fork.setup.liveSaved'));
    } catch (error) {
      toast.error(String(error));
    }
  };

  return (
    <MeetingSetupContext.Provider value={{ requestSetup, liveContext, openLiveEditor }}>
      {children}
      <Dialog
        open={mode !== null}
        onOpenChange={open => {
          if (!open) {
            if (mode === 'setup') finishSetup(null);
            else setMode(null);
          }
        }}
      >
        <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>{mode === 'live' ? t('fork.setup.liveTitle') : t('fork.setup.title')}</DialogTitle>
            <DialogDescription>{mode === 'live' ? t('fork.setup.liveDescription') : t('fork.setup.description')}</DialogDescription>
          </DialogHeader>

          {mode === 'setup' && (
            <div className="space-y-2">
              <div className="text-sm font-medium text-gray-800">{t('fork.setup.meetingTitle')}</div>
              <Input
                autoFocus
                placeholder={t('fork.setup.meetingTitlePlaceholder')}
                value={title}
                onChange={e => setTitle(e.target.value)}
              />
            </div>
          )}

          <MeetingContextForm value={draft} onChange={setDraft} showInstructions={false} showScreenCapture />

          <DialogFooter className="gap-2">
            {mode === 'setup' ? (
              <>
                <Button variant="ghost" onClick={() => finishSetup(null)}>{t('fork.common.cancel')}</Button>
                <Button
                  variant="outline"
                  onClick={() => finishSetup({ title: title.trim() || null, context: { participants: [], captureScreen: draft.captureScreen } })}
                >
                  {t('fork.setup.startWithoutContext')}
                </Button>
                <Button onClick={() => finishSetup({ title: title.trim() || null, context: cleanContext(draft) })}>
                  {t('fork.setup.start')}
                </Button>
              </>
            ) : (
              <>
                <Button variant="ghost" onClick={() => setMode(null)}>{t('fork.common.cancel')}</Button>
                <Button onClick={saveLive}>{t('fork.common.save')}</Button>
              </>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </MeetingSetupContext.Provider>
  );
}

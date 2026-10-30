'use client';

import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { invoke } from '@tauri-apps/api/core';
import { MeetingContextForm } from '@/components/fork/MeetingContextForm';
import { RecordingSourcesPicker, type RecordingSources } from '@/components/fork/RecordingSourcesPicker';
import { useConfig } from '@/contexts/ConfigContext';
import { configService } from '@/services/configService';
import type { ResumeRequest } from '@/lib/fork/resume';
import {
  getMeetingContext,
  getPreferences,
  PREF,
  setPreferences,
  setLiveMeetingContext,
  startScreenCapture,
  stopScreenCapture,
  type MeetingContext,
} from '@/services/forkService';

export interface MeetingSetupResult {
  /** Title typed by the user, or null to keep the generated one */
  title: string | null;
  context: MeetingContext;
  /** Microphone and meeting audio chosen for this recording (null = default / automatic) */
  devices: { micDevice: string | null; systemDevice: string | null };
}

export interface SetupOptions {
  /** Resume this stopped meeting: its context is pre-filled, its title kept */
  resume?: ResumeRequest;
}

interface MeetingSetupContextValue {
  /** Ask for title + context before a recording; null if the user cancelled. */
  requestSetup: (options?: SetupOptions) => Promise<MeetingSetupResult | null>;
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

/**
 * The microphone and screen chosen at start become the defaults for the next meeting.
 * The meeting audio output is never remembered (Conseil 28/10): a fixed output silently
 * records nothing once the call plays elsewhere (23/10 incident).
 */
async function rememberSources(sources: RecordingSources, previousScreen: string) {
  try {
    const prefs = await configService.getRecordingPreferences();
    if (prefs.preferred_mic_device !== sources.micDevice) {
      await invoke('set_recording_preferences', { preferences: { ...prefs, preferred_mic_device: sources.micDevice } });
    }
    if (sources.screenMonitor !== previousScreen) {
      await setPreferences({ [PREF.screenMonitor]: sources.screenMonitor });
    }
  } catch (error) {
    console.warn('Could not remember the recording sources:', error);
  }
}

export function MeetingSetupProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<'setup' | 'live' | null>(null);
  const [title, setTitle] = useState('');
  const [draft, setDraft] = useState<MeetingContext>({ participants: [] });
  const [liveContext, setLiveContext] = useState<MeetingContext | null>(null);
  const [resumeOf, setResumeOf] = useState<ResumeRequest | null>(null);
  const resolverRef = useRef<((result: MeetingSetupResult | null) => void) | null>(null);
  // Fork: microphone, meeting audio and screen for the recording being started
  const { selectedDevices, setSelectedDevices } = useConfig();
  const [sources, setSources] = useState<RecordingSources>({ micDevice: null, systemDevice: null, screenMonitor: 'primary' });
  const savedScreenRef = useRef('primary');

  // Forget the live context once the recording is over (the backend saved it with the meeting)
  useEffect(() => {
    const unlisten = listen('recording-stopped', () => setLiveContext(null));
    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  const requestSetup = useCallback(async (options?: SetupOptions) => {
    // A second start request (button + sidebar) cancels the first one instead of leaving it pending
    resolverRef.current?.(null);
    resolverRef.current = null;
    let captureDefault = false;
    let screenMonitor = 'primary';
    try {
      const prefs = await getPreferences();
      captureDefault = prefs[PREF.screenDefaultEnabled] === 'true';
      screenMonitor = prefs[PREF.screenMonitor] || 'primary';
    } catch {
      // defaults below
    }
    savedScreenRef.current = screenMonitor;
    // Meeting audio always starts on Automatique (never remembered)
    setSources({ micDevice: selectedDevices?.micDevice ?? null, systemDevice: null, screenMonitor });
    const selfName = readSelfName();
    setResumeOf(options?.resume ?? null);
    let initial: MeetingContext = {
      meetingType: 'visio',
      participants: selfName ? [{ name: selfName, isSelf: true }] : [],
      captureScreen: captureDefault,
    };
    if (options?.resume) {
      // Resuming: start from the meeting's own context
      try {
        const saved = await getMeetingContext(options.resume.meetingId);
        initial = { ...saved, captureScreen: saved.captureScreen ?? captureDefault };
      } catch (error) {
        console.warn('Could not load the context of the resumed meeting:', error);
      }
    }
    setTitle(options?.resume?.title ?? '');
    setDraft(initial);
    setMode('setup');
    return new Promise<MeetingSetupResult | null>(resolve => {
      resolverRef.current = resolve;
    });
  }, [selectedDevices]);

  const finishSetup = (result: MeetingSetupResult | null) => {
    resolverRef.current?.(result);
    resolverRef.current = null;
    setMode(null);
    setResumeOf(null);
    if (result) {
      rememberSelfName(result.context);
      setLiveContext(result.context);
      setSelectedDevices({ micDevice: result.devices.micDevice, systemDevice: selectedDevices?.systemDevice ?? null });
      void rememberSources(sources, savedScreenRef.current);
    }
  };

  const startWith = (context: MeetingContext) =>
    finishSetup({
      title: resumeOf ? resumeOf.title : title.trim() || null,
      context: { ...context, screenMonitor: context.captureScreen ? sources.screenMonitor : null },
      devices: { micDevice: sources.micDevice, systemDevice: sources.systemDevice },
    });

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
            <DialogTitle>
              {mode === 'live'
                ? t('fork.setup.liveTitle')
                : resumeOf
                  ? t('fork.resume.dialogTitle', { title: resumeOf.title })
                  : t('fork.setup.title')}
            </DialogTitle>
            <DialogDescription>
              {mode === 'live'
                ? t('fork.setup.liveDescription')
                : resumeOf
                  ? t('fork.resume.dialogWarning')
                  : t('fork.setup.description')}
            </DialogDescription>
          </DialogHeader>

          {mode === 'setup' && !resumeOf && (
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

          {mode === 'setup' && (
            <RecordingSourcesPicker
              value={sources}
              onChange={setSources}
              captureScreen={!!draft.captureScreen}
              meetingType={draft.meetingType}
            />
          )}

          <DialogFooter className="gap-2">
            {mode === 'setup' ? (
              <>
                <Button variant="ghost" onClick={() => finishSetup(null)}>{t('fork.common.cancel')}</Button>
                {!resumeOf && (
                  <Button variant="outline" onClick={() => startWith({ participants: [], captureScreen: draft.captureScreen })}>
                    {t('fork.setup.startWithoutContext')}
                  </Button>
                )}
                <Button onClick={() => startWith(cleanContext(draft))}>
                  {resumeOf ? t('fork.resume.start') : t('fork.setup.start')}
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

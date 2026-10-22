"use client";
import { useState, useEffect, useRef, useMemo } from 'react';
import { motion } from 'framer-motion';
import { MeetingSummary, SummaryProcessResponse, Transcript } from '@/types';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { MeetingDetailsSplitView, type MeetingDetailsTab } from '@/components/MeetingDetails/MeetingDetailsSplitView';
import { ModelConfig } from '@/components/ModelSettingsModal';

// Custom hooks
import { useMeetingData } from '@/hooks/meeting-details/useMeetingData';
import { useSummaryGeneration } from '@/hooks/meeting-details/useSummaryGeneration';
import { useTemplates } from '@/hooks/meeting-details/useTemplates';
import { useCopyOperations } from '@/hooks/meeting-details/useCopyOperations';
import { useMeetingOperations } from '@/hooks/meeting-details/useMeetingOperations';
import { useConfig } from '@/contexts/ConfigContext';
import { useMeetingContextData } from '@/hooks/fork/useMeetingContextData';
import { useMeetingSpeakers } from '@/hooks/fork/useMeetingSpeakers';
import { usePostMeetingPipeline } from '@/hooks/fork/usePostMeetingPipeline';
import { MeetingCloudBar } from '@/components/fork/MeetingCloudBar';
import { MeetingContextPanel } from '@/components/fork/MeetingContextPanel';
import { IllustrateSummaryButton } from '@/components/fork/IllustrateSummaryButton';
import { SegmentPlayButton, SpeakerChip } from '@/components/fork/SpeakerChip';

export default function PageContent({
  meeting,
  summaryData,
  initialSummary,
  shouldAutoGenerate = false,
  fromRecording = false,
  onAutoGenerateComplete,
  onMeetingUpdated,
  onRefetchTranscripts,
  // Pagination props for efficient transcript loading
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
}: {
  meeting: any;
  summaryData: MeetingSummary | null;
  initialSummary: SummaryProcessResponse | null;
  shouldAutoGenerate?: boolean;
  fromRecording?: boolean;
  onAutoGenerateComplete?: () => void;
  onMeetingUpdated?: () => Promise<void>;
  onRefetchTranscripts?: () => Promise<void>;
  // Pagination props
  segments?: any[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;
}) {
  const { t } = useTranslation();
  console.log('📄 PAGE CONTENT: Initializing with data:', {
    meetingId: meeting.id,
    summaryDataKeys: summaryData ? Object.keys(summaryData) : null,
    transcriptsCount: meeting.transcripts?.length
  });

  // Fork: persisted context (its instructions are the summary's custom prompt),
  // speaker names and the post-recording cloud steps
  const meetingContext = useMeetingContextData(meeting.id);
  const customPrompt = meetingContext.context.extraInstructions ?? '';
  const setCustomPrompt = (value: string) =>
    meetingContext.update({ ...meetingContext.context, extraInstructions: value });
  const meetingSpeakers = useMeetingSpeakers(meeting.id, () => onRefetchTranscripts?.());
  const pipeline = usePostMeetingPipeline({
    meetingId: meeting.id,
    fromRecording,
    onTranscriptsChanged: async () => {
      await onRefetchTranscripts?.();
      await meetingSpeakers.reload();
    },
  });

  const isRecording = false;
  const [activeTab, setActiveTab] = useState<MeetingDetailsTab>('transcript');

  // Ref to store the modal open function from SummaryGeneratorButtonGroup
  const openModelSettingsRef = useRef<(() => void) | null>(null);
  const autoSwitchedSummaryMeetingIdsRef = useRef(new Set<string>());
  const manuallySelectedTabMeetingIdsRef = useRef(new Set<string>());
  const autoGenerationStartedMeetingIdRef = useRef<string | null>(null);

  // Sidebar context
  const { serverAddress } = useSidebar();

  // Get model config from ConfigContext
  const { modelConfig, setModelConfig, isModelConfigLoading } = useConfig();

  // Custom hooks
  const meetingData = useMeetingData({ meeting, summaryData, onMeetingUpdated });
  // Every speaker key of the meeting (targets of "same speaker as...")
  const speakerKeys = useMemo(() => {
    const keys = new Set<string>(Object.keys(meetingSpeakers.speakers));
    (segments ?? []).forEach(s => s.speaker && keys.add(s.speaker));
    (meetingData.transcripts as Transcript[]).forEach(tr => tr.speaker && keys.add(tr.speaker));
    keys.delete('mixed');
    return [...keys].sort();
  }, [meetingSpeakers.speakers, segments, meetingData.transcripts]);
  const templates = useTemplates();

  // Callback to register the modal open function
  const handleRegisterModalOpen = (openFn: () => void) => {
    console.log('📝 Registering modal open function in PageContent');
    openModelSettingsRef.current = openFn;
  };

  // Callback to trigger modal open (called from error handler)
  const handleOpenModelSettings = () => {
    console.log('🔔 Opening model settings from PageContent');
    if (openModelSettingsRef.current) {
      openModelSettingsRef.current();
    } else {
      console.warn('⚠️ Modal open function not yet registered');
    }
  };

  // Save model config to backend database and sync via event
  const handleSaveModelConfig = async (config?: ModelConfig) => {
    if (!config) return;
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey ?? null,
        ollamaEndpoint: config.ollamaEndpoint ?? null,
      });

      // Emit event so ConfigContext and other listeners stay in sync
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      toast.success(t('summary.model.saved'));
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error(t('summary.model.saveError'));
    }
  };

  const summaryGeneration = useSummaryGeneration({
    initialSummary,
    meeting,
    transcripts: meetingData.transcripts,
    modelConfig: modelConfig,
    isModelConfigLoading,
    selectedTemplate: templates.selectedTemplate,
    onMeetingUpdated,
    updateMeetingTitle: meetingData.updateMeetingTitle,
    setAiSummary: meetingData.setAiSummary,
    onOpenModelSettings: handleOpenModelSettings,
  });

  const copyOperations = useCopyOperations({
    meeting,
    transcripts: meetingData.transcripts,
    meetingTitle: meetingData.meetingTitle,
    aiSummary: meetingData.aiSummary,
    blockNoteSummaryRef: meetingData.blockNoteSummaryRef,
  });

  const meetingOperations = useMeetingOperations({
    meeting,
  });

  // Track page view
  useEffect(() => {
    Analytics.trackPageView('meeting_details');
  }, []);

  useEffect(() => {
    if (
      (meetingData.aiSummary || summaryGeneration.summaryStatus === 'completed')
      && !autoSwitchedSummaryMeetingIdsRef.current.has(meeting.id)
      && !manuallySelectedTabMeetingIdsRef.current.has(meeting.id)
    ) {
      autoSwitchedSummaryMeetingIdsRef.current.add(meeting.id);
      setActiveTab('summary');
    }
  }, [meeting.id, meetingData.aiSummary, summaryGeneration.summaryStatus]);

  // Auto-generate only after the model configuration has settled.
  useEffect(() => {
    if (
      !shouldAutoGenerate
      || !pipeline.isDone
      || summaryGeneration.summaryStatus !== 'idle'
      || isModelConfigLoading
      || meetingData.transcripts.length === 0
      || autoGenerationStartedMeetingIdRef.current === meeting.id
    ) {
      return;
    }

    autoGenerationStartedMeetingIdRef.current = meeting.id;
    console.log(`🤖 Auto-generating summary with ${modelConfig.provider}/${modelConfig.model}...`);
    onAutoGenerateComplete?.();
    // Fork: save a pending context edit first, the backend reads it from the database
    void meetingContext.flush().then(() => summaryGeneration.handleGenerateSummary(''));
  }, [
    shouldAutoGenerate,
    pipeline.isDone,
    meeting.id,
    meetingData.transcripts.length,
    isModelConfigLoading,
    modelConfig.provider,
    modelConfig.model,
    summaryGeneration.handleGenerateSummary,
    summaryGeneration.summaryStatus,
    onAutoGenerateComplete,
  ]);

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-col h-screen min-w-0 bg-gray-50"
    >
      <div className="flex flex-1 min-w-0 overflow-hidden">
        <MeetingDetailsSplitView
          activeTab={activeTab}
          onTabChange={(tab) => {
            manuallySelectedTabMeetingIdsRef.current.add(meeting.id);
            setActiveTab(tab);
          }}
          transcript={
            <TranscriptPanel
              transcripts={meetingData.transcripts}
              customPrompt={customPrompt}
              onPromptChange={setCustomPrompt}
              onCopyTranscript={copyOperations.handleCopyTranscript}
              onOpenMeetingFolder={meetingOperations.handleOpenMeetingFolder}
              isRecording={isRecording}
              disableAutoScroll={true}
              usePagination={true}
              segments={segments}
              hasMore={hasMore}
              isLoadingMore={isLoadingMore}
              totalCount={totalCount}
              loadedCount={loadedCount}
              onLoadMore={onLoadMore}
              meetingId={meeting.id}
              meetingFolderPath={meeting.folder_path}
              onRefetchTranscripts={onRefetchTranscripts}
              topBar={<MeetingCloudBar meetingId={meeting.id} pipeline={pipeline} />}
              contextPanel={
                <MeetingContextPanel
                  context={meetingContext.context}
                  onChange={meetingContext.update}
                  saving={meetingContext.saving}
                />
              }
              renderSpeaker={segment => (
                <>
                <SegmentPlayButton meetingId={meeting.id} segment={{ id: segment.id, start: segment.timestamp, end: segment.endTime }} />
                <SpeakerChip
                  segment={{ id: segment.id, start: segment.timestamp, end: segment.endTime }}
                  speakerKeys={speakerKeys}
                  onMerge={meetingSpeakers.mergeSpeaker}
                  onReassign={meetingSpeakers.reassignSegment}
                  speakerKey={segment.speaker}
                  speakers={meetingSpeakers.speakers}
                  onRename={meetingSpeakers.renameSpeaker}
                />
                </>
              )}
            />
          }
          summary={
            <SummaryPanel
              meeting={meeting}
              meetingTitle={meetingData.meetingTitle}
              summaryRef={meetingData.blockNoteSummaryRef}
              isSaving={meetingData.isSaving}
              isSummaryDirty={meetingData.isSummaryDirty}
              onSaveAll={meetingData.saveAllChanges}
              onCopySummary={copyOperations.handleCopySummary}
              aiSummary={meetingData.aiSummary}
              summaryStatus={summaryGeneration.summaryStatus}
              transcripts={meetingData.transcripts}
              modelConfig={modelConfig}
              setModelConfig={setModelConfig}
              onSaveModelConfig={handleSaveModelConfig}
              onGenerateSummary={async (prompt?: string) => {
                await meetingContext.flush();
                return summaryGeneration.handleGenerateSummary(prompt);
              }}
              onStopGeneration={summaryGeneration.handleStopGeneration}
              customPrompt={customPrompt}
              onSaveSummary={meetingData.handleSaveSummary}
              onSummaryChange={meetingData.handleSummaryChange}
              onDirtyChange={meetingData.setIsSummaryDirty}
              summaryError={summaryGeneration.summaryError}
              onRegenerateSummary={async () => {
                await meetingContext.flush();
                return summaryGeneration.handleRegenerateSummary();
              }}
              getSummaryStatusMessage={summaryGeneration.getSummaryStatusMessage}
              availableTemplates={templates.availableTemplates}
              selectedTemplate={templates.selectedTemplate}
              onTemplateSelect={templates.handleTemplateSelection}
              isModelConfigLoading={isModelConfigLoading}
              extraActions={
                <IllustrateSummaryButton
                  meetingId={meeting.id}
                  summaryRef={meetingData.blockNoteSummaryRef}
                  disabled={
                    ['processing', 'summarizing', 'regenerating'].includes(summaryGeneration.summaryStatus) ||
                    pipeline.stage === 'diarizing' ||
                    pipeline.stage === 'vision'
                  }
                />
              }
              onOpenModelSettings={handleRegisterModalOpen}
            />
          }
        />
      </div>
    </motion.div>
  );
}

'use client';

import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, ChevronUp, NotebookPen } from 'lucide-react';
import type { MeetingContext } from '@/services/forkService';
import { MeetingContextForm } from './MeetingContextForm';

/**
 * Persisted context of a meeting (replaces upstream's unsaved "context for AI" box):
 * participants, background, glossary and instructions for the summary.
 */
export function MeetingContextPanel({
  context,
  onChange,
  saving,
}: {
  context: MeetingContext;
  onChange: (context: MeetingContext) => void;
  saving?: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const participants = context.participants.filter(p => p.name.trim());
  const summary = [
    participants.length ? t('fork.contextPanel.participants', { count: participants.length }) : null,
    context.contextText?.trim() ? t('fork.contextPanel.hasBackground') : null,
    context.extraInstructions?.trim() ? t('fork.contextPanel.hasInstructions') : null,
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <div className="border-t border-gray-200 bg-white">
      <button
        type="button"
        onClick={() => setOpen(o => !o)}
        className="flex w-full items-center gap-2 px-4 py-2 text-left hover:bg-gray-50"
      >
        <NotebookPen className="w-4 h-4 text-gray-500" />
        <span className="text-sm font-medium text-gray-800">{t('fork.contextPanel.title')}</span>
        <span className="truncate text-xs text-gray-500">{summary || t('fork.contextPanel.empty')}</span>
        {saving && <span className="text-xs text-gray-400">{t('fork.common.saving')}</span>}
        <span className="ml-auto text-gray-400">{open ? <ChevronDown className="w-4 h-4" /> : <ChevronUp className="w-4 h-4" />}</span>
      </button>
      {open && (
        <div className="max-h-[50vh] overflow-y-auto px-4 pb-4">
          <MeetingContextForm value={context} onChange={onChange} showInstructions />
          <p className="mt-3 text-xs text-gray-500">{t('fork.contextPanel.hint')}</p>
        </div>
      )}
    </div>
  );
}

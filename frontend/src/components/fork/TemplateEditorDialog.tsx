'use client';

import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { ArrowDown, ArrowUp, Eye, Plus, Trash2 } from 'lucide-react';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { previewTemplate, saveCustomTemplate, type Template, type TemplateFormat, type TemplateSection } from '@/services/forkService';

const emptySection = (): TemplateSection => ({ title: '', instruction: '', format: 'paragraph' });

export const emptyTemplate = (): Template => ({ name: '', description: '', sections: [emptySection()] });

/**
 * Create or edit a summary template. Saving writes a custom template: editing a
 * built-in one creates a customised copy under the same id (restorable).
 */
export function TemplateEditorDialog({
  open,
  onOpenChange,
  templateId,
  initial,
  onSaved,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** null = new template */
  templateId: string | null;
  initial: Template;
  onSaved: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<Template>(initial);
  const [preview, setPreview] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open) {
      setDraft(initial);
      setPreview(null);
    }
  }, [open, initial]);

  const updateSection = (index: number, patch: Partial<TemplateSection>) =>
    setDraft(d => ({ ...d, sections: d.sections.map((s, i) => (i === index ? { ...s, ...patch } : s)) }));

  const moveSection = (index: number, delta: number) =>
    setDraft(d => {
      const sections = [...d.sections];
      const target = index + delta;
      if (target < 0 || target >= sections.length) return d;
      [sections[index], sections[target]] = [sections[target], sections[index]];
      return { ...d, sections };
    });

  const showPreview = async () => {
    try {
      const result = await previewTemplate(draft);
      setPreview(result.valid ? `⚠️ ${result.valid}` : result.markdown);
    } catch (error) {
      setPreview(String(error));
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      const cleaned: Template = {
        ...draft,
        sections: draft.sections.map(s => ({ ...s, item_format: s.item_format?.trim() ? s.item_format : undefined })),
      };
      const id = await saveCustomTemplate(templateId, cleaned);
      toast.success(t('fork.templates.saved'));
      onSaved(id);
      onOpenChange(false);
    } catch (error) {
      toast.error(String(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{templateId ? t('fork.templates.editTitle') : t('fork.templates.newTitle')}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="grid grid-cols-1 gap-3">
            <Input placeholder={t('fork.templates.name')} value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} />
            <Input
              placeholder={t('fork.templates.description')}
              value={draft.description}
              onChange={e => setDraft({ ...draft, description: e.target.value })}
            />
          </div>

          <div className="space-y-3">
            {draft.sections.map((section, index) => (
              <div key={index} className="rounded-md border border-gray-200 p-3 space-y-2 bg-gray-50">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-medium text-gray-500 w-6">{index + 1}.</span>
                  <Input
                    placeholder={t('fork.templates.sectionTitle')}
                    value={section.title}
                    onChange={e => updateSection(index, { title: e.target.value })}
                  />
                  <Select value={section.format} onValueChange={v => updateSection(index, { format: v as TemplateFormat })}>
                    <SelectTrigger className="w-36">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="paragraph">{t('fork.templates.formats.paragraph')}</SelectItem>
                      <SelectItem value="list">{t('fork.templates.formats.list')}</SelectItem>
                      <SelectItem value="string">{t('fork.templates.formats.string')}</SelectItem>
                    </SelectContent>
                  </Select>
                  <Button variant="ghost" size="icon" onClick={() => moveSection(index, -1)} disabled={index === 0} title={t('fork.templates.moveUp')}>
                    <ArrowUp />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => moveSection(index, 1)}
                    disabled={index === draft.sections.length - 1}
                    title={t('fork.templates.moveDown')}
                  >
                    <ArrowDown />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => setDraft(d => ({ ...d, sections: d.sections.filter((_, i) => i !== index) }))}
                    disabled={draft.sections.length === 1}
                    title={t('fork.templates.removeSection')}
                  >
                    <Trash2 />
                  </Button>
                </div>
                <Textarea
                  placeholder={t('fork.templates.instruction')}
                  value={section.instruction}
                  onChange={e => updateSection(index, { instruction: e.target.value })}
                  className="min-h-[60px] bg-white"
                />
                <Textarea
                  placeholder={t('fork.templates.itemFormat')}
                  value={section.item_format ?? ''}
                  onChange={e => updateSection(index, { item_format: e.target.value })}
                  className="min-h-[40px] font-mono text-xs bg-white"
                />
              </div>
            ))}
            <Button variant="outline" size="sm" onClick={() => setDraft(d => ({ ...d, sections: [...d.sections, emptySection()] }))}>
              <Plus /> {t('fork.templates.addSection')}
            </Button>
          </div>

          {preview && (
            <pre className="whitespace-pre-wrap rounded-md bg-gray-900 text-gray-100 p-3 text-xs max-h-60 overflow-y-auto">{preview}</pre>
          )}
        </div>

        <DialogFooter className="gap-2">
          <Button variant="outline" onClick={showPreview}>
            <Eye /> {t('fork.templates.preview')}
          </Button>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t('fork.common.cancel')}
          </Button>
          <Button onClick={save} disabled={saving || !draft.name.trim() || !draft.description.trim()}>
            {t('fork.common.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

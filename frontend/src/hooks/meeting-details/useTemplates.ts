import { useState, useEffect, useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';
import Analytics from '@/lib/analytics';
import { getPreferences, PREF } from '@/services/forkService';

// Fork: the default template comes from Settings → Templates (app_preferences)
const FALLBACK_TEMPLATE = 'fr_reunion_standard';

export function useTemplates() {
  const { t } = useTranslation();
  const [availableTemplates, setAvailableTemplates] = useState<Array<{
    id: string;
    name: string;
    description: string;
  }>>([]);
  const [selectedTemplate, setSelectedTemplate] = useState<string>(FALLBACK_TEMPLATE);

  // Fetch available templates and the default one on mount
  useEffect(() => {
    const fetchTemplates = async () => {
      try {
        const templates = await invokeTauri('api_list_templates') as Array<{
          id: string;
          name: string;
          description: string;
        }>;
        // French templates first
        templates.sort((a, b) => Number(b.id.startsWith('fr_')) - Number(a.id.startsWith('fr_')) || a.name.localeCompare(b.name));
        setAvailableTemplates(templates);

        const prefs = await getPreferences();
        const preferred = prefs[PREF.summaryDefaultTemplate];
        if (preferred && templates.some(tpl => tpl.id === preferred)) {
          setSelectedTemplate(preferred);
        }
      } catch (error) {
        console.error('Failed to fetch templates:', error);
      }
    };
    fetchTemplates();
  }, []);

  // Handle template selection
  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    setSelectedTemplate(templateId);
    toast.success(t('summary.templateSelected'), {
      description: t('summary.templateSelectedDescription', { name: templateName }),
    });
    Analytics.trackFeatureUsed('template_selected');
  }, [t]);

  return {
    availableTemplates,
    selectedTemplate,
    handleTemplateSelection,
  };
}

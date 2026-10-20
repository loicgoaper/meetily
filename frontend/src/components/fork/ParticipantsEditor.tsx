'use client';

import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Trash2, UserRound } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { searchContacts, type Contact, type Participant } from '@/services/forkService';

/** Participant list with autocomplete on people already met (contacts table). */
export function ParticipantsEditor({
  participants,
  onChange,
}: {
  participants: Participant[];
  onChange: (participants: Participant[]) => void;
}) {
  const { t } = useTranslation();
  const listId = useId();
  const [query, setQuery] = useState('');
  const [contacts, setContacts] = useState<Contact[]>([]);

  useEffect(() => {
    const handle = setTimeout(() => {
      searchContacts(query)
        .then(setContacts)
        .catch(() => setContacts([]));
    }, 150);
    return () => clearTimeout(handle);
  }, [query]);

  const update = (index: number, patch: Partial<Participant>) => {
    onChange(participants.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  };

  const setName = (index: number, name: string) => {
    setQuery(name);
    const known = contacts.find(c => c.name === name);
    const current = participants[index];
    update(index, {
      name,
      role: current.role || known?.role || null,
      company: current.company || known?.company || null,
    });
  };

  const setSelf = (index: number) => {
    onChange(participants.map((p, i) => ({ ...p, isSelf: i === index ? !p.isSelf : false })));
  };

  return (
    <div className="space-y-2">
      {participants.map((p, index) => (
        <div key={index} className="flex items-center gap-2">
          <Input
            className="flex-[2]"
            list={listId}
            placeholder={t('fork.participants.name')}
            value={p.name}
            onChange={e => setName(index, e.target.value)}
          />
          <Input
            className="flex-[1.5]"
            placeholder={t('fork.participants.role')}
            value={p.role ?? ''}
            onChange={e => update(index, { role: e.target.value })}
          />
          <Input
            className="flex-[1.5]"
            placeholder={t('fork.participants.company')}
            value={p.company ?? ''}
            onChange={e => update(index, { company: e.target.value })}
          />
          <Button
            type="button"
            variant={p.isSelf ? 'blue' : 'ghost'}
            size="icon"
            title={t('fork.participants.isSelf')}
            onClick={() => setSelf(index)}
          >
            <UserRound />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            title={t('fork.participants.remove')}
            onClick={() => onChange(participants.filter((_, i) => i !== index))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <datalist id={listId}>
        {contacts.map(c => (
          <option key={c.id} value={c.name}>
            {[c.role, c.company].filter(Boolean).join(' · ')}
          </option>
        ))}
      </datalist>
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => onChange([...participants, { name: '', isSelf: participants.length === 0 }])}
      >
        <Plus /> {t('fork.participants.add')}
      </Button>
      <p className="text-xs text-gray-500">{t('fork.participants.hint')}</p>
    </div>
  );
}
